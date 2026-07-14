use std::{
  borrow::Cow,
  io::{BufRead, BufReader},
  path::Path,
  process::{Child, Command, Stdio},
  str::from_utf8,
  sync::{mpsc::Sender, Arc, LazyLock, Mutex},
  thread,
};

use anyhow::{anyhow, Result};

use super::{
  progress::{
    FfmpegParser, JobProgress, LoudnormResult, ProgressInfo, StepType,
  },
  settings::RenderSettings,
  track::{AudioRenderer, TrackStats},
};

type SharedChild = Arc<Mutex<Option<Child>>>;

pub(crate) static ACTIVE_CHILDREN: LazyLock<Mutex<Vec<SharedChild>>> =
  LazyLock::new(|| Mutex::new(Vec::new()));

pub fn register_child(handle: SharedChild) {
  if let Ok(mut lock) = ACTIVE_CHILDREN.lock() {
    lock.push(handle);
  }
}

pub fn deregister_child(handle: &SharedChild) {
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
    // Append -progress - to arguments to output structured progress reports to stdout
    let mut final_args = args.to_vec();
    final_args.push("-progress".to_string());
    final_args.push("-".to_string());
    cmd
      .args(&final_args)
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

    let duration = Arc::new(Mutex::new(None));
    let duration_clone = Arc::clone(&duration);
    let tx_clone = tx.clone();
    let step_clone = step.clone();

    // Spawn a background thread to read stdout and parse progress reports
    let stdout_thread = thread::spawn(move || {
      let reader = BufReader::new(stdout_pipe);
      let mut progress_info = ProgressInfo::new();

      for line in reader.lines() {
        let line = match line {
          Ok(l) => l,
          Err(_) => break, // process killed or closed pipe
        };

        if progress_info.parse_line(&line) {
          let dur_opt = *duration_clone.lock().unwrap();
          let pct = if let Some(dur) = dur_opt {
            if dur > 0.0 {
              let current_secs =
                progress_info.out_time_us.unwrap_or(0) as f32 / 1_000_000.0;
              AudioRenderer::clamp(current_secs / dur, 0.0, 1.0)
            } else {
              0.0
            }
          } else {
            0.0
          };

          let _ = tx_clone.send(JobProgress::Progress {
            step: step_clone.clone(),
            percent: pct,
            speed: progress_info.speed.clone(),
            time_str: progress_info.out_time.clone(),
          });
        }
      }
    });

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

        let mut dur_lock = duration.lock().unwrap();
        if dur_lock.is_none() {
          if let Some(d) = FfmpegParser::parse_duration(line) {
            *dur_lock = Some(d);
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

  fn run_mix_computation(
    &self,
    input_file: &str,
    settings: &RenderSettings,
    tx: &Sender<JobProgress>,
  ) -> Result<Vec<f32>> {
    let _ = tx.send(JobProgress::Starting(StepType::MixComputation));
    let _ = tx.send(JobProgress::Log(Arc::from(
      "Starting Step [1/3]: Mix Computation...",
    )));

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
    let res = self.run_command(
      &settings.ffmpeg_path,
      &mix_args,
      StepType::MixComputation,
      tx,
      &mut |line| {
        if let Some((idx, is_mean, val)) =
          FfmpegParser::parse_volume_detect(line)
        {
          if idx < track_stats.len() {
            if is_mean {
              track_stats[idx].mean = Some(val);
            } else {
              track_stats[idx].peak = Some(val);
            }
          }
        }
      },
    );

    if let Err(e) = res {
      let _ = tx.send(JobProgress::Failed(Arc::from(format!(
        "Step 1 Failed: {}",
        e
      ))));
      return Err(e);
    }

    // Print parsed values
    for (i, t) in track_stats.iter().enumerate() {
      let name = &audio_tracks[i].name;
      let mean_str = t
        .mean
        .map(|v| format!("{:.1} dBFS", v))
        .unwrap_or_else(|| "-".to_string());
      let peak_str = t
        .peak
        .map(|v| format!("{:.1} dBFS", v))
        .unwrap_or_else(|| "-".to_string());
      let _ = tx.send(JobProgress::Log(Arc::from(format!(
        "  {}  mean: {}   peak: {}",
        name, mean_str, peak_str
      ))));
    }

    if let Some(computed) =
      AudioRenderer::compute_mix_volumes(&settings.audio, &track_stats)
    {
      let _ =
        tx.send(JobProgress::Log(Arc::from("Computed volume adjustments:")));
      for (i, vol) in computed.iter().enumerate() {
        let name = &audio_tracks[i].name;
        let mean = track_stats[i].mean.unwrap();
        let _ = tx.send(JobProgress::Log(Arc::from(format!(
          "  {:<9} {:.1}dB  →  {:.1} dBFS mean",
          &**name,
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
    let _ = tx.send(JobProgress::Starting(StepType::AudioAnalysis));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "Starting Step [{}/{}]: Audio Analysis...",
      step_num, total_steps
    ))));

    let mut analysis_args = vec!["-i".to_string(), input_file.to_string()];
    if let Some(vols) = volumes {
      let mix_filter = AudioRenderer::build_mix_filter_complex(
        &settings.audio.tracks,
        vols,
        "loudnorm=I=-14:LRA=11:TP=-1:print_format=json",
      );
      analysis_args.push("-filter_complex".to_string());
      analysis_args.push(mix_filter);
    } else {
      analysis_args.push("-af".to_string());
      analysis_args
        .push("loudnorm=I=-14:LRA=11:TP=-1:print_format=json".to_string());
    }
    analysis_args.push("-f".to_string());
    analysis_args.push("null".to_string());
    analysis_args.push("-".to_string());

    let mut input_i = None;
    let mut input_lra = None;
    let mut input_tp = None;
    let mut input_thresh = None;
    let mut target_offset = None;

    let raw_analysis = self.run_command(
      &settings.ffmpeg_path,
      &analysis_args,
      StepType::AudioAnalysis,
      tx,
      &mut |line| {
        if input_i.is_none() {
          input_i = FfmpegParser::extract_loudnorm_val(line, "\"input_i\"");
        }
        if input_lra.is_none() {
          input_lra = FfmpegParser::extract_loudnorm_val(line, "\"input_lra\"");
        }
        if input_tp.is_none() {
          input_tp = FfmpegParser::extract_loudnorm_val(line, "\"input_tp\"");
        }
        if input_thresh.is_none() {
          input_thresh =
            FfmpegParser::extract_loudnorm_val(line, "\"input_thresh\"");
        }
        if target_offset.is_none() {
          target_offset =
            FfmpegParser::extract_loudnorm_val(line, "\"target_offset\"");
        }
      },
    );

    if let Err(e) = raw_analysis {
      let _ = tx.send(JobProgress::Failed(Arc::from(format!(
        "Step {} Failed: {}",
        step_num, e
      ))));
      return Err(e);
    }

    let res = match (input_i, input_lra, input_tp, input_thresh, target_offset)
    {
      (
        Some(input_i),
        Some(input_lra),
        Some(input_tp),
        Some(input_thresh),
        Some(target_offset),
      ) => LoudnormResult {
        input_i,
        input_lra,
        input_tp,
        input_thresh,
        target_offset,
      },
      _ => {
        let err_msg =
          "Failed to parse loudnorm JSON output from ffmpeg.".to_string();
        let _ = tx.send(JobProgress::Failed(Arc::from(err_msg.clone())));
        return Err(anyhow!(err_msg));
      }
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
    let _ = tx.send(JobProgress::Starting(StepType::VideoEncoding));
    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "Starting Step [{}/{}]: Video Encoding...",
      step_num, total_steps
    ))));

    let output_file_str = output_file.to_string();

    let mut encode_args =
      vec!["-y".to_string(), "-i".to_string(), input_file.to_string()];
    if let Some(vols) = volumes {
      let loudnorm_suffix = format!(
        "loudnorm=I=-14:LRA=11:TP=-1:measured_I={}:measured_LRA={}:measured_TP={}:measured_thresh={}:offset={}:linear=true[out]",
        res.input_i, res.input_lra, res.input_tp, res.input_thresh, res.target_offset
      );
      let mix_filter = AudioRenderer::build_mix_filter_complex(
        &settings.audio.tracks,
        vols,
        &loudnorm_suffix,
      );
      encode_args.push("-filter_complex".to_string());
      encode_args.push(mix_filter);
      encode_args.push("-map".to_string());
      encode_args.push("0:v:0".to_string());
      encode_args.push("-map".to_string());
      encode_args.push("[out]".to_string());
    } else {
      encode_args.push("-af".to_string());
      encode_args.push(format!(
        "loudnorm=I=-14:LRA=11:TP=-1:measured_I={}:measured_LRA={}:measured_TP={}:measured_thresh={}:offset={}:linear=true",
        res.input_i, res.input_lra, res.input_tp, res.input_thresh, res.target_offset
      ));
    }

    encode_args.extend(settings.custom_vflags.iter().map(|s| s.to_string()));
    encode_args.push(output_file_str.clone());

    let res_encode = self.run_command(
      &settings.ffmpeg_path,
      &encode_args,
      StepType::VideoEncoding,
      tx,
      &mut |_| {},
    );

    if let Err(e) = res_encode {
      let _ = tx.send(JobProgress::Failed(Arc::from(format!(
        "Step {} Failed: {}",
        step_num, e
      ))));
      return Err(e);
    }

    let _ = tx.send(JobProgress::Log(Arc::from(format!(
      "Output at {}",
      output_file_str
    ))));
    let _ = tx.send(JobProgress::Completed(Arc::from(output_file_str)));

    Ok(())
  }

  pub fn execute(
    &self,
    input_file: &str,
    output_file: &str,
    settings: &RenderSettings,
    tx: Sender<JobProgress>,
  ) -> Result<()> {
    let input_path = Path::new(input_file);
    if !input_path.exists() {
      let _ = tx.send(JobProgress::Failed(Arc::from(format!(
        "Input file not found: {}",
        input_file
      ))));
      return Err(anyhow!("Input file not found"));
    }

    let single_track = settings.audio.single_track;
    let total_steps = if single_track { 2 } else { 3 };

    let volumes = if !single_track {
      let vols = self.run_mix_computation(input_file, settings, &tx)?;
      Some(vols)
    } else {
      None
    };

    let analysis_step_num = if single_track { 1 } else { 2 };
    let loudnorm_res = self.run_audio_analysis(
      input_file,
      settings,
      volumes.as_deref(),
      analysis_step_num,
      total_steps,
      &tx,
    )?;

    let encode_step_num = if single_track { 2 } else { 3 };
    self.run_video_encoding(
      input_file,
      output_file,
      settings,
      &loudnorm_res,
      volumes.as_deref(),
      encode_step_num,
      total_steps,
      &tx,
    )?;

    Ok(())
  }
}

impl Default for RenderProcess {
  fn default() -> Self {
    Self::new()
  }
}
