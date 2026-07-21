use serde::{Deserialize, Serialize};

use crate::ffmpeg::{settings::TrackConfig, AudioSettings};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TrackStats {
  pub mean: Option<f32>,
  pub peak: Option<f32>,
}

pub struct AudioRenderer;

impl AudioRenderer {
  pub fn clamp(val: f32, min: f32, max: f32) -> f32 {
    if val < min {
      min
    } else if val > max {
      max
    } else {
      val
    }
  }

  pub fn build_mix_filter_complex(
    tracks: &[TrackConfig],
    volumes: &[f32],
    loudnorm_suffix: &str,
  ) -> String {
    let mut filter_parts = Vec::new();
    let mut amix_inputs = Vec::new();
    for (i, track) in tracks.iter().enumerate() {
      let idx = track.index;
      let label = track.name.to_lowercase();
      let vol = volumes.get(i).copied().unwrap_or(0.0);
      filter_parts.push(format!("[0:a:{idx}]volume={vol:.1}dB[{label}]"));
      amix_inputs.push(format!("[{label}]"));
    }
    let amix_inputs_str = amix_inputs.join("");
    let n_inputs = tracks.len();
    let weights = vec!["1"; n_inputs].join(" ");
    let suffix = if loudnorm_suffix.contains("[out]") {
      loudnorm_suffix.to_string()
    } else {
      format!("{loudnorm_suffix}[out]")
    };
    format!(
      "{};{}amix=inputs={}:weights='{}':dropout_transition=2:normalize=0[mixed];[mixed]{}",
      filter_parts.join(";"),
      amix_inputs_str,
      n_inputs,
      weights,
      suffix
    )
  }

  pub fn append_filter_args(
    args: &mut Vec<String>,
    tracks: &[TrackConfig],
    volumes: Option<&[f32]>,
    loudnorm_config: &str,
  ) {
    if let Some(vols) = volumes {
      let mix_filter =
        Self::build_mix_filter_complex(tracks, vols, loudnorm_config);
      args.push("-filter_complex".to_string());
      args.push(mix_filter);
      args.push("-map".to_string());
      args.push("0:v:0".to_string());
      args.push("-map".to_string());
      args.push("[out]".to_string());
    } else {
      args.push("-af".to_string());
      args.push(loudnorm_config.to_string());
    }
  }

  pub fn compute_mix_volumes(
    settings: &AudioSettings,
    track_stats: &[TrackStats],
  ) -> Option<Vec<f32>> {
    let tracks = &settings.tracks;
    if tracks.is_empty() {
      return None;
    }

    let threshold = -45.0;

    let all_means_present = tracks
      .iter()
      .enumerate()
      .all(|(i, _)| i < track_stats.len() && track_stats[i].mean.is_some());

    if !all_means_present {
      return None;
    }

    let mut computed_vols = vec![0.0_f32; tracks.len()];
    let mut ref_posts = vec![0.0_f32; tracks.len()];

    // First track: apply its offset directly
    let first_offset = tracks[0].offset;
    let vol = Self::clamp(first_offset, -100.0, 30.0);
    let mean = track_stats[0].mean.unwrap();
    let ref_post = if mean >= threshold {
      mean + vol
    } else {
      -20.0 + vol
    };
    computed_vols[0] = vol;
    ref_posts[0] = ref_post;

    // Remaining tracks: each relative to the previous
    for i in 1..tracks.len() {
      let offset = tracks[i].offset;
      let prev_offset = tracks[i - 1].offset;
      let prev_ref_post = ref_posts[i - 1];
      let mean = track_stats[i].mean.unwrap();
      let target = prev_ref_post + (offset - prev_offset);
      let (vol, ref_post) = if mean >= threshold {
        let v = Self::clamp(target - mean, -100.0, 30.0);
        (v, mean + v)
      } else {
        let v = Self::clamp(offset, -100.0, 30.0);
        (v, target)
      };
      computed_vols[i] = vol;
      ref_posts[i] = ref_post;
    }

    Some(computed_vols)
  }
}
