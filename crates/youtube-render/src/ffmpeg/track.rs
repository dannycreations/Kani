use crate::ffmpeg::{settings::TrackConfig, AudioSettings};

#[cfg(test)]
#[path = "track_test.rs"]
mod track_test;

const MIN_GAIN_DB: f32 = -100.0;
const MAX_GAIN_DB: f32 = 30.0;
const SILENCE_FLOOR_DB: f32 = -45.0;
const SILENCE_REFERENCE_DB: f32 = -20.0;

#[derive(Debug, Default, Clone)]
pub struct TrackStats {
  pub mean: Option<f32>,
  pub peak: Option<f32>,
}

pub struct AudioRenderer;

impl AudioRenderer {
  fn build_mix_filter_complex(
    tracks: &[TrackConfig],
    volumes: &[f32],
    loudnorm_suffix: &str,
  ) -> String {
    let mut inputs = String::new();
    let mut filter_parts = Vec::new();
    for (i, track) in tracks.iter().enumerate() {
      let label = track.name.to_lowercase();
      let vol = volumes.get(i).copied().unwrap_or(0.0);
      filter_parts
        .push(format!("[0:a:{}]volume={vol:.1}dB[{label}]", track.index));
      inputs.push_str(&format!("[{label}]"));
    }

    format!(
      "{};{inputs}amix=inputs={}:weights='{}':dropout_transition=2:normalize=0[mixed];[mixed]{loudnorm_suffix}[out]",
      filter_parts.join(";"),
      tracks.len(),
      vec!["1"; tracks.len()].join(" "),
    )
  }

  pub fn append_filter_args(
    args: &mut Vec<String>,
    tracks: &[TrackConfig],
    volumes: Option<&[f32]>,
    loudnorm_config: &str,
  ) {
    match volumes {
      Some(vols) => {
        args.push("-filter_complex".to_string());
        args.push(Self::build_mix_filter_complex(
          tracks,
          vols,
          loudnorm_config,
        ));
        args.push("-map".to_string());
        args.push("0:v:0".to_string());
        args.push("-map".to_string());
        args.push("[out]".to_string());
      }
      None => {
        args.push("-af".to_string());
        args.push(loudnorm_config.to_string());
      }
    }
  }

  pub fn compute_mix_volumes(
    settings: &AudioSettings,
    track_stats: &[TrackStats],
  ) -> Option<Vec<f32>> {
    if settings.tracks.is_empty() {
      return None;
    }

    let mut volumes = Vec::with_capacity(settings.tracks.len());
    let mut reference_level: Option<f32> = None;
    let mut previous_offset = 0.0;

    for (i, track) in settings.tracks.iter().enumerate() {
      let mean = track_stats.get(i)?.mean?;
      let offset_gain = track.offset.clamp(MIN_GAIN_DB, MAX_GAIN_DB);

      let (gain, new_reference) = match reference_level {
        None => {
          let level = if mean >= SILENCE_FLOOR_DB {
            mean
          } else {
            SILENCE_REFERENCE_DB
          };
          (offset_gain, level + offset_gain)
        }
        Some(reference) => {
          let target = reference + (track.offset - previous_offset);
          if mean >= SILENCE_FLOOR_DB {
            let gain = (target - mean).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
            (gain, mean + gain)
          } else {
            (offset_gain, target)
          }
        }
      };

      volumes.push(gain);
      reference_level = Some(new_reference);
      previous_offset = track.offset;
    }

    Some(volumes)
  }
}
