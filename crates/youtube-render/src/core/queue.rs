use std::{
  path::{Path, PathBuf},
  sync::{mpsc::channel, Arc, Mutex},
  thread,
};

use serde::{Deserialize, Serialize};

use crate::ffmpeg::{
  AudioSettings, JobProgress, Preset, RenderProcess, RenderSettings,
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

fn compute_output_path(
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
    if let Some(pos) = self.active_processes.iter().position(|(j, _)| *j == id)
    {
      let (_, proc) = self.active_processes.remove(pos);
      proc.cancel();
    }
    self.queue.retain(|item| item.id != id);
  }

  pub fn stop(&mut self) {
    self.is_running = false;
    for (job_id, proc) in self.active_processes.drain(..) {
      proc.cancel();
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
      Self::pump_queue(state_arc);
    });
  }

  pub fn pump_queue(state_arc: Arc<Mutex<AppState>>) {
    let mut state = state_arc.lock().unwrap();
    if !state.is_running {
      return;
    }

    let max_jobs = if state.enable_parallel {
      state.parallel_jobs.max(1)
    } else {
      1
    };

    while state.active_processes.len() < max_jobs {
      let next_pos = state
        .queue
        .iter()
        .position(|item| matches!(item.status, QueueItemStatus::Pending));

      if let Some(pos) = next_pos {
        let job_id = state.queue[pos].id;
        let proc = Arc::new(RenderProcess::new());
        state.active_processes.push((job_id, Arc::clone(&proc)));

        let ffmpeg_path = Arc::clone(&state.ffmpeg_path);

        let item = &mut state.queue[pos];
        item.status = QueueItemStatus::Processing {
          step: Arc::from("Starting..."),
          percent: 0.0,
          speed: Arc::from(""),
          time_str: Arc::from(""),
        };
        item.logs.clear();

        let input_path = Arc::clone(&item.input_path);
        let output_path = Arc::clone(&item.output_path);
        let settings = RenderSettings {
          audio: item.settings.clone(),
          ffmpeg_path,
          custom_vflags: RenderSettings::default().custom_vflags,
        };

        let tx_state_arc = Arc::clone(&state_arc);
        thread::spawn(move || {
          let (tx, rx) = channel();
          let proc_clone = Arc::clone(&proc);
          let input_clone = Arc::clone(&input_path);
          let output_clone = Arc::clone(&output_path);

          let handle = thread::spawn(move || {
            let _ =
              proc_clone.execute(&input_clone, &output_clone, &settings, tx);
          });

          while let Ok(progress) = rx.recv() {
            let mut state = tx_state_arc.lock().unwrap();
            if !state.is_running
              || !state.active_processes.iter().any(|(id, _)| *id == job_id)
            {
              break;
            }

            if let Some(item) =
              state.queue.iter_mut().find(|item| item.id == job_id)
            {
              match progress {
                JobProgress::Starting(step_type) => {
                  item.status = QueueItemStatus::Processing {
                    step: Arc::from(step_type.name()),
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
                  item.status = QueueItemStatus::Processing {
                    step: Arc::from(step.name()),
                    percent,
                    speed: speed.unwrap_or_default(),
                    time_str: time_str.unwrap_or_default(),
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

          let _ = handle.join();

          {
            let mut state = tx_state_arc.lock().unwrap();
            state.active_processes.retain(|(id, _)| *id != job_id);
            if let Some(item) =
              state.queue.iter_mut().find(|item| item.id == job_id)
            {
              if let QueueItemStatus::Processing { .. } = item.status {
                item.status = QueueItemStatus::Cancelled;
                item.logs.push(Arc::from("Job cancelled or stopped."));
              }
            }

            let has_pending = state
              .queue
              .iter()
              .any(|item| matches!(item.status, QueueItemStatus::Pending));
            if !has_pending && state.active_processes.is_empty() {
              state.is_running = false;
            }
          }

          Self::pump_queue(tx_state_arc);
        });
      } else {
        break;
      }
    }
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

    // Per-item settings default check (from default preset, sorted by index)
    assert!(!state.queue[0].settings.single_track);
    assert_eq!(state.queue[0].settings.tracks.len(), 3);
    assert_eq!(&*state.queue[0].settings.tracks[0].name, "Game");
    assert_eq!(state.queue[0].settings.tracks[0].offset, -16.0);
    assert_eq!(&*state.queue[0].settings.tracks[2].name, "Discord");
    assert_eq!(state.queue[0].settings.tracks[2].offset, -6.0);

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
