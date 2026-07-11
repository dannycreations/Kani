use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::track::AudioTrack;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSettings {
  pub single_track: bool,
  pub game_offset: f32,
  pub mic_offset: f32,
  pub discord_offset: f32,
}

impl AudioSettings {
  pub fn get_offset(&self, track: AudioTrack) -> f32 {
    match track {
      AudioTrack::Mic => self.mic_offset,
      AudioTrack::Discord => self.discord_offset,
      AudioTrack::Game => self.game_offset,
    }
  }

  pub fn set_offset(&mut self, track: AudioTrack, offset: f32) {
    match track {
      AudioTrack::Mic => self.mic_offset = offset,
      AudioTrack::Discord => self.discord_offset = offset,
      AudioTrack::Game => self.game_offset = offset,
    }
  }
}

impl Default for AudioSettings {
  fn default() -> Self {
    Self {
      single_track: false,
      game_offset: AudioTrack::Game.default_offset(),
      mic_offset: AudioTrack::Mic.default_offset(),
      discord_offset: AudioTrack::Discord.default_offset(),
    }
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
