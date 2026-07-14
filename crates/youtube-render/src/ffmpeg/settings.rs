use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::ffmpeg::{ini::IniSerializer, preset::Preset};

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
      custom_vflags: Arc::from(
        vec![
          Arc::from("-c:v"),
          Arc::from("h264_nvenc"),
          Arc::from("-profile:v"),
          Arc::from("high"),
          Arc::from("-tune"),
          Arc::from("hq"),
          Arc::from("-preset"),
          Arc::from("p7"),
          Arc::from("-rc"),
          Arc::from("vbr"),
          Arc::from("-b:v"),
          Arc::from("15M"),
          Arc::from("-maxrate"),
          Arc::from("20M"),
          Arc::from("-bufsize"),
          Arc::from("40M"),
          Arc::from("-g"),
          Arc::from("120"),
          Arc::from("-bf"),
          Arc::from("2"),
          Arc::from("-rc-lookahead"),
          Arc::from("32"),
          Arc::from("-spatial_aq"),
          Arc::from("1"),
          Arc::from("-temporal_aq"),
          Arc::from("1"),
          Arc::from("-aq-strength"),
          Arc::from("8"),
          Arc::from("-r"),
          Arc::from("60"),
          Arc::from("-pix_fmt"),
          Arc::from("yuv420p"),
          Arc::from("-colorspace"),
          Arc::from("bt709"),
          Arc::from("-color_primaries"),
          Arc::from("bt709"),
          Arc::from("-color_trc"),
          Arc::from("bt709"),
          Arc::from("-color_range"),
          Arc::from("tv"),
          Arc::from("-c:a"),
          Arc::from("aac"),
          Arc::from("-b:a"),
          Arc::from("384k"),
          Arc::from("-ar"),
          Arc::from("48000"),
          Arc::from("-movflags"),
          Arc::from("+faststart"),
        ]
        .into_boxed_slice(),
      ),
    }
  }
}
