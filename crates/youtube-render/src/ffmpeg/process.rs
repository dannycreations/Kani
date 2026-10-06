use std::{
  borrow::Cow,
  io::{BufRead, BufReader},
  path::Path,
  process::{Child, Command, Stdio},
  str::from_utf8,
  sync::{mpsc::Sender, Arc, LazyLock, Mutex, OnceLock},
  thread,
};

use anyhow::{anyhow, Result};

use crate::{
  core::DEFAULT_LOUDNORM_CONFIG,
  ffmpeg::{
    progress::{
      FfmpegParser, JobProgress, LoudnormResult, ProgressInfo, StepType,
      VolumeType,
    },
    settings::RenderSettings,
    track::{AudioRenderer, TrackStats},
  },
};

const MIX_STEP_NUM: usize = 1;

type SharedChild = Arc<Mutex<Option<Child>>>;

pub(crate) static ACTIVE_CHILDREN: LazyLock<Mutex<Vec<SharedChild>>> =
  LazyLock::new(|| Mutex::new(Vec::new()));

pub fn register_child(handle: SharedChild) {
  if let Ok(mut lock) = ACTIVE_CHILDREN.lock() {
    lock.push(handle);
  }
}

fn deregister_child(handle: &SharedChild) {
  if let Ok(mut lock) = ACTIVE_CHILDREN.lock() {
    lock.retain(|h| !Arc::ptr_eq(h, handle));
  }
}

pub fn kill_all_children() {
  if let Ok(mut lock) = ACTIVE_CHILDREN.lock() {
    for handle in lock.drain(..) {
      let mut child_lock = handle.lock().unwrap();
      if let Some(mut child) = child_lock.take() {
        let _ = child.kill();
      }
    }
  }
}

struct ChildRegistrationGuard(SharedChild);

impl Drop for ChildRegistrationGuard {
  fn drop(&mut self) {
    deregister_child(&self.0);
  }
}

pub struct RenderProcess {
  child_handle: Arc<Mutex<Option<Child>>>,
}

impl Drop for RenderProcess {
  fn drop(&mut self) {
    self.cancel();
  }
}

impl RenderProcess {
  pub fn new() -> Self {
    Self {
      child_handle: Arc::new(Mutex::new(None)),
    }
  }

  pub fn cancel(&self) {
    let mut lock = self.child_handle.lock().unwrap();
    if let Some(mut child) = lock.take() {
      let _ = child.kill();
    }
  }

  fn run_command(
    &self,
    ffmpeg_path: &str,
    args: &[String],
    step: StepType,
    tx: &Sender<JobProgress>,
    on_line: &mut dyn FnMut(&str),
  ) -> Result<()> {
    let mut cmd = Command::new(ffmpeg_path);
    cmd
      .args(args)
      // Ask ffmpeg for structured progress reports on stdout.
      .args(["-progress", "-"])
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|e| {
      anyhow!(
        "Failed to start ffmpeg: {}. Check if ffmpeg is in your PATH.",
        e
      )
    })?;

    // Keep track of child so it can be cancelled
    {
      let mut lock = self.child_handle.lock().unwrap();
      *lock = Some(child);
    }

    // Register with global active children
    register_child(Arc::clone(&self.child_handle));
    let _guard = ChildRegistrationGuard(Arc::clone(&self.child_handle));

    // Fetch stderr and stdout as child is running
    let (stdout_pipe, stderr_pipe) = {
      let mut lock = self.child_handle.lock().unwrap();
      let child_ref = lock
        .as_mut()
        .ok_or_else(|| anyhow!("Job was cancelled before execution."))?;
      let stdout = child_ref
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to open stdout pipe."))?;
      let stderr = child_ref
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to open stderr pipe."))?;
      (stdout, stderr)
    };

    // ffmpeg reports the media duration once on stderr; the stdout reader needs it
    // to turn timestamps into a percentage.
    let duration = Arc::new(OnceLock::<f32>::new());
    let stdout_thread = {
      let duration = Arc::clone(&duration);
      let tx = tx.clone();
      let step = step.clone();

      thread::spawn(move || {
        let reader = BufReader::new(stdout_pipe);
        let mut progress_info = ProgressInfo::new();

        for line in reader.lines() {
          let Ok(line) = line else {
            break; // process killed or closed pipe
          };

          if !progress_info.parse_line(&line) {
            continue;
          }

          let percent = match duration.get().copied() {
            Some(total) if total > 0.0 => {
              let elapsed =
                progress_info.out_time_us.unwrap_or(0) as f32 / 1_000_000.0;
              (elapsed / total).clamp(0.0, 1.0)
            }
            _ => 0.0,
          };

          let _ = tx.send(JobProgress::Progress {
            step: step.clone(),
            percent,
            speed: progress_info.speed.clone(),
            time_str: progress_info.out_time.clone(),
          });
        }
      })
    };

    let mut reader = BufReader::new(stderr_pipe);
    let mut buf = Vec::new();

    loop {
      buf.clear();
      // Read until either a newline '\n' or carriage return '\r' is encountered.
      // This is crucial because ffmpeg outputs volume detection lines and errors ending in '\n' or '\r'.
      let bytes_read = match reader.read_until(b'\n', &mut buf) {
        Ok(0) => break, // EOF
        Ok(n) => n,
        Err(_) => break, // process killed or closed pipe
      };

      // Split the read chunk by carriage return '\r' in case multiple lines are bundled
      let chunk = match from_utf8(&buf[..bytes_read]) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => String::from_utf8_lossy(&buf[..bytes_read]),
      };
      for line in chunk.split('\r') {
        if line.is_empty() {
          continue;
        }

        on_line(line);

        if duration.get().is_none() {
          if let Some(d) = FfmpegParser::parse_duration(line) {
            let _ = duration.set(d);
          }
        }
      }
    }

    // Wait for stdout parsing thread to finish
    let _ = stdout_thread.join();

    // Wait for process completion
    let status = {
      let mut lock = self.child_handle.lock().unwrap();
      if let Some(mut child) = lock.take() {
        child.wait()?
      } else {
        return Err(anyhow!("Job was cancelled."));
      }
    };

    if !status.success() {
      return Err(anyhow!(
        "ffmpeg exited with error code: {:?}",
        status.code()
      ));
    }

    Ok(())
  }

  #[allow(clippy::too_many_arguments)]
  fn run_step(
    &self,
    ffmpeg_path: &str,
    args: &[String],
    step: StepType,
    step_num: usize,
    total_steps: usize,
    tx: &Sender<JobProgress>,
    on_line: &mut dyn FnMut(&str),
  ) -> Result<()> {
    let _ = tx.send(JobProgress::Starting(step.clone()));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "Starting Step [{}/{}]: {}...",
      step_num,
      total_steps,
      step.name()
    ))));

    self
      .run_command(ffmpeg_path, args, step, tx, on_line)
      .inspect_err(|e| {
        let _ = tx.send(JobProgress::Failed(Arc::from(format!(
          "Step {} Failed: {}",
          step_num, e
        ))));
      })
  }

  fn run_mix_computation(
    &self,
    input_file: &str,
    settings: &RenderSettings,
    total_steps: usize,
    tx: &Sender<JobProgress>,
  ) -> Result<Vec<f32>> {
    let audio_tracks = &settings.audio.tracks;
    let track_count = audio_tracks.len();

    // Build volumedetect filter chain dynamically from preset tracks
    let filter = audio_tracks
      .iter()
      .enumerate()
      .map(|(i, t)| format!("[0:a:{}]volumedetect[a{}]", t.index, i))
      .collect::<Vec<_>>()
      .join(";");

    let mut mix_args = vec![
      "-i".to_string(),
      input_file.to_string(),
      "-filter_complex".to_string(),
      filter,
    ];
    for i in 0..track_count {
      mix_args.push("-map".to_string());
      mix_args.push(format!("[a{}]", i));
    }
    mix_args.push("-f".to_string());
    mix_args.push("null".to_string());
    mix_args.push("-".to_string());

    let mut track_stats = vec![TrackStats::default(); track_count];
    self.run_step(
      &settings.ffmpeg_path,
      &mix_args,
      StepType::MixComputation,
      MIX_STEP_NUM,
      total_steps,
      tx,
      &mut |line| {
        let Some(info) = FfmpegParser::parse_volume_detect(line) else {
          return;
        };
        let Some(stats) = track_stats.get_mut(info.track_index) else {
          return;
        };
        match info.volume_type {
          VolumeType::Mean => stats.mean = Some(info.volume_db),
          VolumeType::Max => stats.peak = Some(info.volume_db),
        }
      },
    )?;

    // Print parsed values
    let show_db = |value: Option<f32>| {
      value.map_or_else(|| "-".to_owned(), |v| format!("{:.1} dBFS", v))
    };
    for (i, stats) in track_stats.iter().enumerate() {
      let name = &audio_tracks[i].name;
      let _ = tx.send(JobProgress::Log(Arc::from(format!(
        "  {}  mean: {}   peak: {}",
        name,
        show_db(stats.mean),
        show_db(stats.peak)
      ))));
    }

    if let Some(computed) =
      AudioRenderer::compute_mix_volumes(&settings.audio, &track_stats)
    {
      let _ =
        tx.send(JobProgress::Log(Arc::from("Computed volume adjustments:")));
      for ((vol, stats), track) in
        computed.iter().zip(&track_stats).zip(audio_tracks)
      {
        let Some(mean) = stats.mean else { continue };
        let _ = tx.send(JobProgress::Log(Arc::from(format!(
          "  {:<9} {:.1}dB  →  {:.1} dBFS mean",
          &*track.name,
          vol,
          mean + vol
        ))));
      }
      Ok(computed)
    } else {
      let _ = tx.send(JobProgress::Log(Arc::from(
        "Warning: track levels unreadable; falling back to 0dB adjustments",
      )));
      Ok(vec![0.0; track_count])
    }
  }

  fn run_audio_analysis(
    &self,
    input_file: &str,
    settings: &RenderSettings,
    volumes: Option<&[f32]>,
    step_num: usize,
    total_steps: usize,
    tx: &Sender<JobProgress>,
  ) -> Result<LoudnormResult> {
    let mut analysis_args = vec!["-i".to_string(), input_file.to_string()];
    AudioRenderer::append_filter_args(
      &mut analysis_args,
      &settings.audio.tracks,
      volumes,
      &format!("{DEFAULT_LOUDNORM_CONFIG}:print_format=json"),
    );
    analysis_args.push("-f".to_string());
    analysis_args.push("null".to_string());
    analysis_args.push("-".to_string());

    let mut input_i = None;
    let mut input_lra = None;
    let mut input_tp = None;
    let mut input_thresh = None;
    let mut target_offset = None;

    self.run_step(
      &settings.ffmpeg_path,
      &analysis_args,
      StepType::AudioAnalysis,
      step_num,
      total_steps,
      tx,
      &mut |line| {
        let slots = [
          ("\"input_i\"", &mut input_i),
          ("\"input_lra\"", &mut input_lra),
          ("\"input_tp\"", &mut input_tp),
          ("\"input_thresh\"", &mut input_thresh),
          ("\"target_offset\"", &mut target_offset),
        ];
        for (key, slot) in slots {
          if slot.is_none() {
            *slot = FfmpegParser::extract_loudnorm_val(line, key);
          }
        }
      },
    )?;

    let require = |value: Option<f32>| -> Result<f32> {
      value.ok_or_else(|| {
        let _ = tx.send(JobProgress::Failed(Arc::from(
          "Failed to parse loudnorm JSON output from ffmpeg.",
        )));
        anyhow!("Failed to parse loudnorm JSON output from ffmpeg.")
      })
    };

    let res = LoudnormResult {
      input_i: require(input_i)?,
      input_lra: require(input_lra)?,
      input_tp: require(input_tp)?,
      input_thresh: require(input_thresh)?,
      target_offset: require(target_offset)?,
    };

    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "  Integrated Loudness (I) : {} LUFS",
      res.input_i
    ))));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "  Loudness Range  (LRA)   : {} LU",
      res.input_lra
    ))));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "  True Peak       (TP)    : {} dB",
      res.input_tp
    ))));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "  Threshold               : {}",
      res.input_thresh
    ))));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "  Offset                  : {}",
      res.target_offset
    ))));

    Ok(res)
  }

  #[allow(clippy::too_many_arguments)]
  fn run_video_encoding(
    &self,
    input_file: &str,
    output_file: &str,
    settings: &RenderSettings,
    res: &LoudnormResult,
    volumes: Option<&[f32]>,
    step_num: usize,
    total_steps: usize,
    tx: &Sender<JobProgress>,
  ) -> Result<()> {
    let mut encode_args =
      vec!["-y".to_string(), "-i".to_string(), input_file.to_string()];
    AudioRenderer::append_filter_args(
      &mut encode_args,
      &settings.audio.tracks,
      volumes,
      &format!(
        "{DEFAULT_LOUDNORM_CONFIG}:measured_I={}:measured_LRA={}:measured_TP={}:measured_thresh={}:offset={}:linear=true",
        res.input_i, res.input_lra, res.input_tp, res.input_thresh, res.target_offset
      ),
    );

    encode_args.extend(settings.custom_vflags.iter().map(|s| s.to_string()));
    encode_args.push(output_file.to_string());

    self.run_step(
      &settings.ffmpeg_path,
      &encode_args,
      StepType::VideoEncoding,
      step_num,
      total_steps,
      tx,
      &mut |_| {},
    )?;

    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "Output at {}",
      output_file
    ))));
    let _ = tx.send(JobProgress::Completed(Arc::from(output_file)));

    Ok(())
  }

  pub fn execute(
    &self,
    input_file: &str,
    output_file: &str,
    settings: &RenderSettings,
    tx: Sender<JobProgress>,
  ) -> Result<()> {
    if !Path::new(input_file).exists() {
      let _ = tx.send(JobProgress::Failed(Arc::from(format!(
        "Input file not found: {}",
        input_file
      ))));
      return Err(anyhow!("Input file not found"));
    }

    let single_track = settings.audio.single_track;
    let total_steps = if single_track { 2 } else { 3 };
    let analysis_step_num = if single_track { MIX_STEP_NUM } else { 2 };

    let volumes = if single_track {
      None
    } else {
      Some(self.run_mix_computation(input_file, settings, total_steps, &tx)?)
    };

    let loudnorm_res = self.run_audio_analysis(
      input_file,
      settings,
      volumes.as_deref(),
      analysis_step_num,
      total_steps,
      &tx,
    )?;

    self.run_video_encoding(
      input_file,
      output_file,
      settings,
      &loudnorm_res,
      volumes.as_deref(),
      analysis_step_num + 1,
      total_steps,
      &tx,
    )?;

    Ok(())
  }
}
