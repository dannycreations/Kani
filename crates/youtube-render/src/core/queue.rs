use std::{
  mem,
  path::{Path, PathBuf},
  sync::{mpsc::channel, Arc, Mutex},
  thread,
};

use crate::ffmpeg::{
  AudioSettings, JobProgress, Preset, RenderProcess, RenderSettings,
};

#[derive(Debug, Clone, PartialEq)]
pub enum QueueItemStatus {
  Pending,
  Processing {
    step: Arc<str>,
    percent: f32,
    speed: Arc<str>,
    time_str: Arc<str>,
  },
  Completed,
  Failed(Arc<str>),
  Cancelled,
}

impl QueueItemStatus {
  fn processing(
    step: &str,
    percent: f32,
    speed: Arc<str>,
    time_str: Arc<str>,
  ) -> Self {
    Self::Processing {
      step: Arc::from(step),
      percent,
      speed,
      time_str,
    }
  }

  pub fn is_pending(&self) -> bool {
    matches!(self, Self::Pending)
  }

  pub fn is_finished(&self) -> bool {
    matches!(self, Self::Completed | Self::Failed(_) | Self::Cancelled)
  }
}

fn compute_output_path(input_path: &str, queue: &[QueueItem]) -> String {
  let mut candidate = PathBuf::from(input_path);
  candidate.set_extension("mp4");

  let taken = |path: &str| {
    Path::new(path).exists()
      || queue.iter().any(|item| &*item.output_path == path)
  };

  let candidate = candidate.to_string_lossy().into_owned();
  if !taken(&candidate) {
    return candidate;
  }

  let parent = Path::new(&candidate)
    .parent()
    .unwrap_or_else(|| Path::new(""));
  let stem = Path::new(&candidate)
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("output");
  let mut suffix = 1;
  loop {
    let candidate = parent.join(format!("{stem}_{suffix}.mp4"));
    let candidate = candidate.to_string_lossy().into_owned();
    if !taken(&candidate) {
      return candidate;
    }
    suffix += 1;
  }
}

#[derive(Debug, Clone)]
pub struct QueueItem {
  pub id: usize,
  pub input_path: Arc<str>,
  pub output_path: Arc<str>,
  pub preset_index: usize,
  pub settings: AudioSettings,
  pub status: QueueItemStatus,
  pub logs: Vec<Arc<str>>,
}

pub struct AppState {
  pub queue: Vec<QueueItem>,
  pub ffmpeg_path: Arc<str>,
  pub enable_parallel: bool,
  pub parallel_jobs: usize,
  pub is_running: bool,
  pub active_processes: Vec<(usize, Arc<RenderProcess>)>,
  pub next_id: usize,
}

impl AppState {
  pub fn new() -> Self {
    Self {
      queue: Vec::new(),
      ffmpeg_path: Arc::from("ffmpeg"),
      enable_parallel: false,
      parallel_jobs: 2,
      is_running: false,
      active_processes: Vec::new(),
      next_id: 1,
    }
  }

  pub fn item(&self, id: usize) -> Option<&QueueItem> {
    self.queue.iter().find(|item| item.id == id)
  }

  pub fn item_mut(&mut self, id: usize) -> Option<&mut QueueItem> {
    self.queue.iter_mut().find(|item| item.id == id)
  }

  pub fn is_job_active(&self, id: usize) -> bool {
    self
      .active_processes
      .iter()
      .any(|(job_id, _)| *job_id == id)
  }

  pub fn add_file(&mut self, path: String) {
    let output_path = compute_output_path(&path, &self.queue);
    let preset_index = 0;
    self.queue.push(QueueItem {
      id: self.next_id,
      input_path: Arc::from(path),
      output_path: Arc::from(output_path),
      preset_index,
      settings: AudioSettings::from_preset(&Preset::builtins()[preset_index]),
      status: QueueItemStatus::Pending,
      logs: Vec::new(),
    });
    self.next_id += 1;
  }

  pub fn remove_item(&mut self, id: usize) {
    if let Some(pos) = self.active_processes.iter().position(|(j, _)| *j == id)
    {
      let (_, proc) = self.active_processes.remove(pos);
      proc.cancel();
    }
    self.queue.retain(|item| item.id != id);
  }

  pub fn stop(&mut self) {
    self.is_running = false;
    let active = mem::take(&mut self.active_processes);
    for (job_id, proc) in active {
      proc.cancel();
      if let Some(item) = self.item_mut(job_id) {
        item.status = QueueItemStatus::Cancelled;
        item.logs.push(Arc::from("Processing stopped by user."));
      }
    }
  }

  pub fn clear_completed(&mut self) {
    self.queue.retain(|item| !item.status.is_finished());
  }

  pub fn clear_all(&mut self) {
    self.stop();
    self.queue.clear();
    self.next_id = 1;
  }

  pub fn move_item(&mut self, id: usize, offset: isize) {
    let Some(from) = self.queue.iter().position(|item| item.id == id) else {
      return;
    };
    let to = from as isize + offset;
    if to < 0 || to as usize >= self.queue.len() {
      return;
    }
    let to = to as usize;

    if self.queue[from].status.is_pending()
      && self.queue[to].status.is_pending()
    {
      self.queue.swap(from, to);
    }
  }

  pub fn start(state: &Arc<Mutex<AppState>>) {
    {
      let mut state = state.lock().unwrap();
      if state.is_running || !state.has_pending() {
        return;
      }
      state.is_running = true;
    }

    Self::pump_queue(state);
  }

  fn has_pending(&self) -> bool {
    self.queue.iter().any(|item| item.status.is_pending())
  }

  fn pump_queue(state: &Arc<Mutex<AppState>>) {
    loop {
      let job = {
        let mut state = state.lock().unwrap();
        if !state.is_running {
          return;
        }
        let max_jobs = if state.enable_parallel {
          state.parallel_jobs.max(1)
        } else {
          1
        };
        if state.active_processes.len() >= max_jobs {
          return;
        }
        match state.claim_next_pending() {
          Some(job) => job,
          None => return,
        }
      };

      // The state lock is released before spawning so the new worker can report
      // progress right away.
      let job_state = Arc::clone(state);
      thread::spawn(move || Self::run_job(job_state, job));
    }
  }

  fn claim_next_pending(&mut self) -> Option<Job> {
    let pos = self
      .queue
      .iter()
      .position(|item| item.status.is_pending())?;
    let item = &mut self.queue[pos];

    item.status =
      QueueItemStatus::processing("Starting...", 0.0, "".into(), "".into());
    item.logs.clear();

    let job = Job {
      id: item.id,
      process: Arc::new(RenderProcess::new()),
      input_path: Arc::clone(&item.input_path),
      output_path: Arc::clone(&item.output_path),
      settings: RenderSettings {
        audio: item.settings.clone(),
        ffmpeg_path: Arc::clone(&self.ffmpeg_path),
      },
    };
    self
      .active_processes
      .push((job.id, Arc::clone(&job.process)));
    Some(job)
  }

  fn run_job(state: Arc<Mutex<AppState>>, job: Job) {
    let Job {
      id,
      process,
      input_path,
      output_path,
      settings,
    } = job;
    let (tx, rx) = channel();

    let worker = thread::spawn(move || {
      // Progress travels over `tx`, so failures are already on screen by the
      // time this returns an error.
      let _ = process.execute(&input_path, &output_path, &settings, tx);
    });

    while let Ok(progress) = rx.recv() {
      let mut state = state.lock().unwrap();
      if !state.is_running || !state.is_job_active(id) {
        break;
      }
      if let Some(item) = state.item_mut(id) {
        item.apply_progress(progress);
      }
    }

    let _ = worker.join();

    {
      let mut state = state.lock().unwrap();
      state.active_processes.retain(|(job_id, _)| *job_id != id);
      if let Some(item) = state.item_mut(id) {
        if let QueueItemStatus::Processing { .. } = item.status {
          item.status = QueueItemStatus::Cancelled;
          item.logs.push(Arc::from("Job cancelled or stopped."));
        }
      }

      if !state.has_pending() && state.active_processes.is_empty() {
        state.is_running = false;
      }
    }

    Self::pump_queue(&state);
  }
}

struct Job {
  id: usize,
  process: Arc<RenderProcess>,
  input_path: Arc<str>,
  output_path: Arc<str>,
  settings: RenderSettings,
}

impl QueueItem {
  fn apply_progress(&mut self, progress: JobProgress) {
    match progress {
      JobProgress::Starting(step) => {
        self.status =
          QueueItemStatus::processing(step.name(), 0.0, "".into(), "".into());
      }
      JobProgress::Log(line) => self.logs.push(line),
      JobProgress::Progress {
        step,
        percent,
        speed,
        time_str,
      } => {
        self.status = QueueItemStatus::processing(
          step.name(),
          percent,
          speed.unwrap_or_default(),
          time_str.unwrap_or_default(),
        );
      }
      JobProgress::Completed => self.status = QueueItemStatus::Completed,
      JobProgress::Failed(error) => {
        self.status = QueueItemStatus::Failed(error)
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn add_three(state: &mut AppState) {
    for name in ["file1.mkv", "file2.mkv", "file3.mkv"] {
      state.add_file(name.to_string());
    }
  }

  fn inputs(state: &AppState) -> Vec<&str> {
    state.queue.iter().map(|item| &*item.input_path).collect()
  }

  fn outputs(state: &AppState) -> Vec<&str> {
    state.queue.iter().map(|item| &*item.output_path).collect()
  }

  #[test]
  fn add_file_seeds_settings_from_the_default_preset() {
    let mut state = AppState::new();
    state.add_file("file1.mkv".to_string());

    let settings = &state.queue[0].settings;
    assert!(!settings.single_track);
    assert_eq!(settings.tracks.len(), 3);
    // The preset lists Mic/Discord/Game, sorted into input-stream order.
    assert_eq!(&*settings.tracks[0].name, "Game");
    assert_eq!(settings.tracks[0].offset, -16.0);
    assert_eq!(&*settings.tracks[2].name, "Discord");
    assert_eq!(settings.tracks[2].offset, -6.0);
  }

  #[test]
  fn each_input_gets_its_own_mp4_output() {
    let mut state = AppState::new();
    add_three(&mut state);
    assert_eq!(outputs(&state), ["file1.mp4", "file2.mp4", "file3.mp4"]);
  }

  #[test]
  fn pending_items_reorder_and_ignore_out_of_range_moves() {
    let mut state = AppState::new();
    add_three(&mut state);

    let second_id = state.queue[1].id;
    state.move_item(second_id, -1);
    assert_eq!(inputs(&state), ["file2.mkv", "file1.mkv", "file3.mkv"]);

    state.move_item(second_id, 1);
    assert_eq!(inputs(&state), ["file1.mkv", "file2.mkv", "file3.mkv"]);

    state.move_item(second_id, 5);
    assert_eq!(inputs(&state), ["file1.mkv", "file2.mkv", "file3.mkv"]);

    state.remove_item(state.queue[0].id);
    assert_eq!(inputs(&state), ["file2.mkv", "file3.mkv"]);
  }

  #[test]
  fn clear_completed_drops_only_finished_items() {
    let mut state = AppState::new();
    add_three(&mut state);

    state.item_mut(state.queue[0].id).unwrap().status =
      QueueItemStatus::Completed;
    state.clear_completed();

    assert_eq!(inputs(&state), ["file2.mkv", "file3.mkv"]);
  }
}
