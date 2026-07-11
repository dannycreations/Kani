use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::settings::AudioSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioTrack {
  Mic,
  Discord,
  Game,
}

impl AudioTrack {
  pub fn all() -> &'static [Self] {
    &[Self::Mic, Self::Discord, Self::Game]
  }

  pub fn display_name(&self) -> &'static str {
    match self {
      Self::Mic => "Mic",
      Self::Discord => "Discord",
      Self::Game => "Game",
    }
  }

  pub fn default_offset(&self) -> f32 {
    match self {
      Self::Mic => -2.0,
      Self::Discord => -6.0,
      Self::Game => -16.0,
    }
  }

  pub fn index(&self) -> usize {
    match self {
      Self::Mic => 1,
      Self::Discord => 2,
      Self::Game => 0,
    }
  }
}

#[derive(Debug, Default, Clone)]
pub struct TrackStats {
  pub mean: Option<f32>,
  pub peak: Option<f32>,
}

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
  tracks: &[AudioTrack],
  computed_vols: &HashMap<AudioTrack, f32>,
  loudnorm_suffix: &str,
) -> String {
  let mut filter_parts = Vec::new();
  let mut amix_inputs = Vec::new();
  for track in tracks {
    let idx = track.index();
    let label = track.display_name().to_lowercase();
    let vol = computed_vols.get(track).copied().unwrap_or(0.0);
    filter_parts.push(format!("[0:a:{idx}]volume={vol:.1}dB[{label}]"));
    amix_inputs.push(format!("[{label}]"));
  }
  let amix_inputs_str = amix_inputs.join("");
  let n_inputs = tracks.len();
  let weights = vec!["1"; n_inputs].join(" ");
  format!(
    "{};{}amix=inputs={}:weights='{}':dropout_transition=2:normalize=0[mixed];[mixed]{}",
    filter_parts.join(";"),
    amix_inputs_str,
    n_inputs,
    weights,
    loudnorm_suffix
  )
}

pub fn compute_mix_volumes(
  settings: &AudioSettings,
  tracks: &[TrackStats],
) -> Option<HashMap<AudioTrack, f32>> {
  let mut computed_vols = HashMap::new();
  let mut ref_posts = HashMap::new();
  let threshold = -45.0;

  let all_means_present = AudioTrack::all().iter().all(|t| {
    let idx = t.index();
    idx < tracks.len() && tracks[idx].mean.is_some()
  });

  if !all_means_present {
    return None;
  }

  let priority_chain = AudioTrack::all(); // [Mic, Discord, Game]
  if !priority_chain.is_empty() {
    let first = priority_chain[0];
    let offset = settings.get_offset(first);
    let vol = clamp(offset, -100.0, 30.0);
    let mean = tracks[first.index()].mean.unwrap();
    let ref_post = if mean >= threshold {
      mean + vol
    } else {
      -20.0 + vol
    };
    computed_vols.insert(first, vol);
    ref_posts.insert(first, ref_post);

    for idx in 1..priority_chain.len() {
      let track = priority_chain[idx];
      let prev_track = priority_chain[idx - 1];
      let offset = settings.get_offset(track);
      let prev_offset = settings.get_offset(prev_track);
      let prev_ref_post = ref_posts[&prev_track];
      let mean = tracks[track.index()].mean.unwrap();
      let target = prev_ref_post + (offset - prev_offset);
      let (vol, ref_post) = if mean >= threshold {
        let v = clamp(target - mean, -100.0, 30.0);
        (v, mean + v)
      } else {
        let v = clamp(offset, -100.0, 30.0);
        (v, target)
      };
      computed_vols.insert(track, vol);
      ref_posts.insert(track, ref_post);
    }
  }

  Some(computed_vols)
}
