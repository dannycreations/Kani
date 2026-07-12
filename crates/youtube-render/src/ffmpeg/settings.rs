use std::{fmt::Write as _, sync::Arc};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::preset::Preset;

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
    Self {
      single_track: false,
      tracks: preset
        .tracks
        .iter()
        .map(|t| TrackConfig {
          name: Arc::clone(&t.name),
          index: t.index,
          offset: t.default_offset,
        })
        .collect(),
    }
  }

  pub fn to_ini(&self) -> String {
    let mut out = String::new();
    out.push_str("[audio]\n");
    let _ = writeln!(out, "single_track = {}", self.single_track);
    out.push('\n');
    for (i, track) in self.tracks.iter().enumerate() {
      let _ = writeln!(out, "[track.{}]", i);
      let _ = writeln!(out, "name = {}", track.name);
      let _ = writeln!(out, "index = {}", track.index);
      let _ = writeln!(out, "offset = {:.1}", track.offset);
      out.push('\n');
    }
    out
  }

  pub fn from_ini(content: &str) -> Result<Self> {
    let mut single_track = false;
    let mut tracks: Vec<TrackConfig> = Vec::new();
    let mut current_section = String::new();
    let mut pending_name: Option<Arc<str>> = None;
    let mut pending_index: Option<usize> = None;
    let mut pending_offset: Option<f32> = None;

    let flush_track = |tracks: &mut Vec<TrackConfig>,
                       name: &mut Option<Arc<str>>,
                       index: &mut Option<usize>,
                       offset: &mut Option<f32>|
     -> Result<()> {
      let n = name
        .take()
        .ok_or_else(|| anyhow!("track section missing 'name'"))?;
      let i = index
        .take()
        .ok_or_else(|| anyhow!("track section missing 'index'"))?;
      let o = offset
        .take()
        .ok_or_else(|| anyhow!("track section missing 'offset'"))?;
      tracks.push(TrackConfig {
        name: n,
        index: i,
        offset: o,
      });
      Ok(())
    };

    for line in content.lines() {
      let line = line.trim();
      if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
        continue;
      }

      // Section header
      if let Some(inner) =
        line.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
      {
        // Flush any pending track before switching sections
        if current_section.starts_with("track.")
          && (pending_name.is_some()
            || pending_index.is_some()
            || pending_offset.is_some())
        {
          flush_track(
            &mut tracks,
            &mut pending_name,
            &mut pending_index,
            &mut pending_offset,
          )?;
        }
        current_section = inner.trim().to_string();
        continue;
      }

      // Key = value
      let Some(eq_pos) = line.find('=') else {
        continue;
      };
      let key = line[..eq_pos].trim();
      let value = line[eq_pos + 1..].trim();

      if current_section == "audio" {
        if key == "single_track" {
          single_track = value == "true";
        }
      } else if current_section.starts_with("track.") {
        match key {
          "name" => pending_name = Some(Arc::from(value)),
          "index" => {
            pending_index = Some(
              value
                .parse::<usize>()
                .map_err(|_| anyhow!("invalid index: '{}'", value))?,
            )
          }
          "offset" => {
            pending_offset = Some(
              value
                .parse::<f32>()
                .map_err(|_| anyhow!("invalid offset: '{}'", value))?,
            )
          }
          _ => {}
        }
      }
    }

    // Flush the last pending track
    if current_section.starts_with("track.")
      && (pending_name.is_some()
        || pending_index.is_some()
        || pending_offset.is_some())
    {
      flush_track(
        &mut tracks,
        &mut pending_name,
        &mut pending_index,
        &mut pending_offset,
      )?;
    }

    Ok(Self {
      single_track,
      tracks,
    })
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
