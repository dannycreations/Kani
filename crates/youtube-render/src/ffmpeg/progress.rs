use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LoudnormResult {
  pub input_i: f32,
  pub input_lra: f32,
  pub input_tp: f32,
  pub input_thresh: f32,
  pub target_offset: f32,
}

pub fn extract_loudnorm_val(line: &str, key: &str) -> Option<f32> {
  let key_pos = line.find(key)?;
  let sub = &line[key_pos + key.len()..];
  let colon_pos = sub.find(':')?;
  let val_part = &sub[colon_pos + 1..];

  // Find the first quote
  let start_quote = val_part.find('"')?;
  let val_part = &val_part[start_quote + 1..];

  // Find the closing quote
  let end_quote = val_part.find('"')?;
  let val_str = &val_part[..end_quote];

  val_str.trim().parse::<f32>().ok()
}

#[derive(Debug, Clone)]
pub enum StepType {
  MixComputation,
  AudioAnalysis,
  VideoEncoding,
}

#[derive(Debug, Clone)]
pub enum JobProgress {
  Starting(StepType),
  Log(Arc<str>),
  Progress {
    step: StepType,
    percent: f32,
    speed: Option<Arc<str>>,
    time_str: Option<Arc<str>>,
  },
  Completed(Arc<str>),
  Failed(Arc<str>),
}

pub fn parse_duration(line: &str) -> Option<f32> {
  let pos = line.find("Duration:")?;
  let sub = line[pos + 9..].split(',').next()?;
  let mut parts = sub.trim().split(':');

  let hours: f32 = parts.next()?.trim().parse().ok()?;
  let minutes: f32 = parts.next()?.trim().parse().ok()?;
  let seconds: f32 = parts.next()?.trim().parse().ok()?;

  if parts.next().is_none() {
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
  } else {
    None
  }
}

pub fn parse_volume_detect(line: &str) -> Option<(usize, bool, f32)> {
  let pos = line.find("volumedetect_")?;
  let sub = &line[pos + 13..];

  // Find where the index digits end
  let end_digits = sub.find(|c: char| !c.is_ascii_digit()).unwrap_or(sub.len());
  if end_digits == 0 {
    return None;
  }
  let idx: usize = sub[..end_digits].parse().ok()?;

  // Check if it contains mean_volume: or max_volume:
  let (is_mean, val_pos) = if let Some(p) = line.find("mean_volume:") {
    (true, p + 12)
  } else {
    let p = line.find("max_volume:")?;
    (false, p + 11)
  };

  let val_sub = &line[val_pos..];
  let val_str = val_sub.split_whitespace().next()?;
  let val: f32 = val_str.parse().ok()?;

  Some((idx, is_mean, val))
}

#[derive(Debug, Clone, Default)]
pub struct ProgressInfo {
  pub out_time_us: Option<i64>,
  pub out_time: Option<Arc<str>>,
  pub speed: Option<Arc<str>>,
}

impl ProgressInfo {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn parse_line(&mut self, line: &str) -> bool {
    let line = line.trim();
    if let Some(val) = line.strip_prefix("out_time_us=") {
      if let Ok(us) = val.trim().parse::<i64>() {
        self.out_time_us = Some(us);
      }
      false
    } else if let Some(val) = line.strip_prefix("out_time=") {
      let trimmed = val.trim();
      let formatted_time = if let Some(dot_idx) = trimmed.find('.') {
        &trimmed[..dot_idx]
      } else {
        trimmed
      };
      self.out_time = Some(Arc::from(formatted_time));
      false
    } else if let Some(val) = line.strip_prefix("speed=") {
      self.speed = Some(Arc::from(val.trim()));
      false
    } else {
      line.starts_with("progress=")
    }
  }
}
