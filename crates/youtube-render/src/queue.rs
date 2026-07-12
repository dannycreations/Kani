use std::{
  path::{Path, PathBuf},
  sync::{mpsc::channel, Arc, Mutex},
  thread,
};

use serde::{Deserialize, Serialize};

use crate::ffmpeg::{
  AudioSettings, JobProgress, Preset, RenderProcess, RenderSettings, StepType,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueueItemStatus {
  Pending,
  Processing {
    step: Arc<str>,
    percent: f32,
    speed: Arc<str>,
    time_str: Arc<str>,
  },
  Completed {
    output_path: Arc<str>,
  },
  Failed(Arc<str>),
  Cancelled,
}

pub fn compute_output_path(
  input_path: &str,
  existing_outputs: &[Arc<str>],
) -> String {
  let mut p = PathBuf::from(input_path);
  p.set_extension("mp4");
  let parent = p.parent().unwrap_or_else(|| Path::new(""));
  let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
  let ext = "mp4";

  let path_exists = |path_str: &str| -> bool {
    Path::new(path_str).exists()
      || existing_outputs.iter().any(|out| &**out == path_str)
  };

  let mut output_str = p.to_string_lossy().to_string();
  if path_exists(&output_str) {
    let mut i = 1;
    loop {
      let candidate = parent.join(format!("{}_{}.{}", stem, i, ext));
      let candidate_str = candidate.to_string_lossy().to_string();
      if !path_exists(&candidate_str) {
        output_str = candidate_str;
        break;
      }
      i += 1;
    }
  }
  output_str
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
  pub id: usize,
  pub input_path: Arc<str>,
  pub output_path: Arc<str>,
  pub preset_index: usize,
  pub settings: AudioSettings,
  pub status: QueueItemStatus,
  pub logs: Vec<Arc<str>>,
}

#[derive(Default)]
pub struct AppState {
  pub queue: Vec<QueueItem>,
  pub ffmpeg_path: Arc<str>,
  pub is_running: bool,
  pub current_job_id: Option<usize>,
  pub active_process: Option<Arc<RenderProcess>>,
  pub next_id: usize,
}

impl AppState {
  pub fn new() -> Self {
    Self {
      queue: Vec::new(),
      ffmpeg_path: Arc::from("ffmpeg"),
      is_running: false,
      current_job_id: None,
      active_process: None,
      next_id: 1,
    }
  }

  pub fn add_file(&mut self, path: String) {
    let existing_outputs: Vec<Arc<str>> = self
      .queue
      .iter()
      .map(|item| Arc::clone(&item.output_path))
      .collect();
    let output_path = compute_output_path(&path, &existing_outputs);
    let preset_index = Preset::default_index();
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
    if self.current_job_id == Some(id) {
      self.stop();
    }
    self.queue.retain(|item| item.id != id);
  }

  pub fn stop(&mut self) {
    self.is_running = false;
    if let Some(proc) = self.active_process.take() {
      proc.cancel();
    }
    if let Some(job_id) = self.current_job_id.take() {
      if let Some(item) = self.queue.iter_mut().find(|item| item.id == job_id) {
        item.status = QueueItemStatus::Cancelled;
        item.logs.push(Arc::from("Processing stopped by user."));
      }
    }
  }

  pub fn clear_completed(&mut self) {
    self.queue.retain(|item| {
      matches!(
        item.status,
        QueueItemStatus::Pending | QueueItemStatus::Processing { .. }
      )
    });
  }

  pub fn clear_all(&mut self) {
    self.stop();
    self.queue.clear();
    self.next_id = 1;
  }

  pub fn move_up(&mut self, id: usize) {
    if let Some(pos) = self.queue.iter().position(|item| item.id == id) {
      if pos > 0
        && matches!(self.queue[pos].status, QueueItemStatus::Pending)
        && matches!(self.queue[pos - 1].status, QueueItemStatus::Pending)
      {
        self.queue.swap(pos, pos - 1);
      }
    }
  }

  pub fn move_down(&mut self, id: usize) {
    if let Some(pos) = self.queue.iter().position(|item| item.id == id) {
      if pos + 1 < self.queue.len()
        && matches!(self.queue[pos].status, QueueItemStatus::Pending)
        && matches!(self.queue[pos + 1].status, QueueItemStatus::Pending)
      {
        self.queue.swap(pos, pos + 1);
      }
    }
  }

  pub fn start(&mut self, state_arc: Arc<Mutex<AppState>>) {
    if self.is_running {
      return;
    }

    // Check if there are any pending items to run
    let has_pending = self
      .queue
      .iter()
      .any(|item| matches!(item.status, QueueItemStatus::Pending));
    if !has_pending {
      return;
    }

    self.is_running = true;

    thread::spawn(move || {
      loop {
        // Determine next job details
        let next_job = {
          let mut state = state_arc.lock().unwrap();
          if !state.is_running {
            break;
          }
          let next_pos = state
            .queue
            .iter()
            .position(|item| matches!(item.status, QueueItemStatus::Pending));
          if let Some(pos) = next_pos {
            let proc = Arc::new(RenderProcess::new());
            state.current_job_id = Some(state.queue[pos].id);
            state.active_process = Some(Arc::clone(&proc));

            let ffmpeg_path = Arc::clone(&state.ffmpeg_path);

            let item = &mut state.queue[pos];
            item.status = QueueItemStatus::Processing {
              step: Arc::from("Starting..."),
              percent: 0.0,
              speed: Arc::from(""),
              time_str: Arc::from(""),
            };
            item.logs.clear();

            let item_id = item.id;
            let input_path = Arc::clone(&item.input_path);
            let output_path = Arc::clone(&item.output_path);
            let settings = RenderSettings {
              audio: item.settings.clone(),
              ffmpeg_path,
              custom_vflags: RenderSettings::default().custom_vflags,
            };
            Some((item_id, input_path, output_path, proc, settings))
          } else {
            state.is_running = false;
            state.current_job_id = None;
            state.active_process = None;
            None
          }
        };

        let (job_id, input_path, output_path, proc, settings) = match next_job {
          Some(j) => j,
          None => break,
        };

        let (tx, rx) = channel();
        let proc_clone = Arc::clone(&proc);
        let input_path_clone = input_path.clone();
        let output_path_clone = output_path.clone();

        // Spawn a runner thread to keep progress channel reactive
        let handle = thread::spawn(move || {
          let _ = proc_clone.execute(
            &input_path_clone,
            &output_path_clone,
            &settings,
            tx,
          );
        });

        // Read progress updates from channel
        while let Ok(progress) = rx.recv() {
          let mut state = state_arc.lock().unwrap();
          if state.current_job_id != Some(job_id) || !state.is_running {
            break;
          }

          if let Some(item) =
            state.queue.iter_mut().find(|item| item.id == job_id)
          {
            match progress {
              JobProgress::Starting(step_type) => {
                let step_name = match step_type {
                  StepType::MixComputation => "Mix Computation",
                  StepType::AudioAnalysis => "Audio Analysis",
                  StepType::VideoEncoding => "Video Encoding",
                };
                item.status = QueueItemStatus::Processing {
                  step: Arc::from(step_name),
                  percent: 0.0,
                  speed: Arc::from(""),
                  time_str: Arc::from(""),
                };
              }
              JobProgress::Log(log_line) => {
                item.logs.push(log_line);
              }
              JobProgress::Progress {
                step,
                percent,
                speed,
                time_str,
              } => {
                let step_name = match step {
                  StepType::MixComputation => "Mix Computation",
                  StepType::AudioAnalysis => "Audio Analysis",
                  StepType::VideoEncoding => "Video Encoding",
                };
                item.status = QueueItemStatus::Processing {
                  step: Arc::from(step_name),
                  percent,
                  speed: speed.unwrap_or_else(|| Arc::from("")),
                  time_str: time_str.unwrap_or_else(|| Arc::from("")),
                };
              }
              JobProgress::Completed(out_path) => {
                item.status = QueueItemStatus::Completed {
                  output_path: out_path,
                };
              }
              JobProgress::Failed(err) => {
                item.status = QueueItemStatus::Failed(err);
              }
            }
          }
        }

        // Wait for the execution thread to finish/cleanup
        let _ = handle.join();

        // Clean up state
        {
          let mut state = state_arc.lock().unwrap();
          state.active_process = None;
          state.current_job_id = None;
          if let Some(item) =
            state.queue.iter_mut().find(|item| item.id == job_id)
          {
            if let QueueItemStatus::Processing { .. } = item.status {
              item.status = QueueItemStatus::Cancelled;
              item.logs.push(Arc::from("Job cancelled or stopped."));
            }
          }
        }
      }
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_queue_operations() {
    let mut state = AppState::new();
    assert_eq!(state.queue.len(), 0);

    // Test add_file
    state.add_file("file1.mkv".to_string());
    state.add_file("file2.mkv".to_string());
    state.add_file("file3.mkv".to_string());
    assert_eq!(state.queue.len(), 3);
    assert_eq!(&*state.queue[0].input_path, "file1.mkv");
    assert_eq!(&*state.queue[1].input_path, "file2.mkv");
    assert_eq!(&*state.queue[2].input_path, "file3.mkv");

    // Per-item settings default check (from default preset)
    assert!(!state.queue[0].settings.single_track);
    assert_eq!(state.queue[0].settings.tracks.len(), 3);
    assert_eq!(&*state.queue[0].settings.tracks[0].name, "Mic");
    assert_eq!(state.queue[0].settings.tracks[0].offset, -2.0);
    assert_eq!(&*state.queue[0].settings.tracks[2].name, "Game");
    assert_eq!(state.queue[0].settings.tracks[2].offset, -16.0);

    // Test output_path uniqueness/incrementing logic (assuming files don't exist on disk)
    assert_eq!(&*state.queue[0].output_path, "file1.mp4");
    assert_eq!(&*state.queue[1].output_path, "file2.mp4");
    assert_eq!(&*state.queue[2].output_path, "file3.mp4");

    // Test move_up
    let second_id = state.queue[1].id;
    state.move_up(second_id);
    assert_eq!(&*state.queue[0].input_path, "file2.mkv");
    assert_eq!(&*state.queue[1].input_path, "file1.mkv");

    // Test move_down
    state.move_down(2);
    assert_eq!(&*state.queue[0].input_path, "file1.mkv");
    assert_eq!(&*state.queue[1].input_path, "file2.mkv");

    // Test remove_item
    state.remove_item(1); // removes file1
    assert_eq!(state.queue.len(), 2);
    assert_eq!(&*state.queue[0].input_path, "file2.mkv");

    // Test clear_completed
    state.queue[0].status = QueueItemStatus::Completed {
      output_path: Arc::from("file2.mp4"),
    };
    state.clear_completed();
    assert_eq!(state.queue.len(), 1);
    assert_eq!(&*state.queue[0].input_path, "file3.mkv");
  }
}
