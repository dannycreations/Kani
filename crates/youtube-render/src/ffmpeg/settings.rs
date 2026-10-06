use std::{fmt::Write as _, sync::Arc};

use anyhow::{anyhow, Result};

use crate::ffmpeg::{ini::IniDocument, preset::Preset};

#[derive(Debug, Clone, PartialEq)]
pub struct TrackConfig {
  pub name: Arc<str>,
  pub index: usize,
  pub offset: f32,
}

#[derive(Debug, Clone, PartialEq)]
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
        name: Arc::from(t.name),
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
    let doc = IniDocument::parse(content);
    let single_track = doc.get("audio", "single_track") == Some("true");

    let mut tracks = Vec::new();
    for i in 0.. {
      let section = format!("track.{}", i);
      if !doc.sections.contains_key(&section) {
        break;
      }

      let name = doc
        .get(&section, "name")
        .ok_or_else(|| anyhow!("track section {} missing 'name'", section))?;
      let index_str = doc
        .get(&section, "index")
        .ok_or_else(|| anyhow!("track section {} missing 'index'", section))?;
      let offset_str = doc
        .get(&section, "offset")
        .ok_or_else(|| anyhow!("track section {} missing 'offset'", section))?;

      let index = index_str.parse::<usize>().map_err(|_| {
        anyhow!("invalid index: '{}' in section {}", index_str, section)
      })?;
      let offset = offset_str.parse::<f32>().map_err(|_| {
        anyhow!("invalid offset: '{}' in section {}", offset_str, section)
      })?;

      tracks.push(TrackConfig {
        name: Arc::from(name),
        index,
        offset,
      });
    }

    Ok(Self {
      single_track,
      tracks,
    })
  }
}

#[derive(Debug, Clone)]
pub struct RenderSettings {
  pub audio: AudioSettings,
  pub ffmpeg_path: Arc<str>,
}
