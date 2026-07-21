use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
  core::DEFAULT_CUSTOM_VFLAGS,
  ffmpeg::{ini::IniSerializer, preset::Preset},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackConfig {
  pub name: Arc<str>,
  pub index: usize,
  pub offset: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSettings {
  pub single_track: bool,
  pub tracks: Vec<TrackConfig>,
}

impl AudioSettings {
  pub fn from_preset(preset: &Preset) -> Self {
    let mut tracks: Vec<TrackConfig> = preset
      .tracks
      .iter()
      .map(|t| TrackConfig {
        name: Arc::clone(&t.name),
        index: t.index,
        offset: t.default_offset,
      })
      .collect();
    tracks.sort_by_key(|t| t.index);
    Self {
      single_track: false,
      tracks,
    }
  }

  pub fn to_ini(&self) -> String {
    IniSerializer::serialize_audio_settings(self)
  }

  pub fn from_ini(content: &str) -> Result<Self> {
    IniSerializer::deserialize_audio_settings(content)
  }
}

impl Default for AudioSettings {
  fn default() -> Self {
    Self::from_preset(&Preset::builtins()[Preset::default_index()])
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSettings {
  pub audio: AudioSettings,
  pub ffmpeg_path: Arc<str>,
  pub custom_vflags: Arc<[Arc<str>]>,
}

impl Default for RenderSettings {
  fn default() -> Self {
    Self {
      audio: AudioSettings::default(),
      ffmpeg_path: Arc::from("ffmpeg"),
      custom_vflags: DEFAULT_CUSTOM_VFLAGS
        .iter()
        .map(|&s| Arc::from(s))
        .collect(),
    }
  }
}
