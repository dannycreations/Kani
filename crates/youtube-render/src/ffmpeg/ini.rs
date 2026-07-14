use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use anyhow::{anyhow, Result};

use crate::ffmpeg::settings::{AudioSettings, TrackConfig};

#[derive(Debug, Default)]
pub struct IniDocument {
  pub sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl IniDocument {
  pub fn parse(content: &str) -> Self {
    let mut doc = Self::default();
    let mut current_section = String::new();

    for line in content.lines() {
      let line = line.trim();
      if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
        continue;
      }

      if let Some(section) =
        line.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
      {
        current_section = section.trim().to_string();
        continue;
      }

      if let Some(eq_pos) = line.find('=') {
        let key = line[..eq_pos].trim().to_string();
        let val = line[eq_pos + 1..].trim().to_string();
        doc
          .sections
          .entry(current_section.clone())
          .or_default()
          .insert(key, val);
      }
    }

    doc
  }

  pub fn get(&self, section: &str, key: &str) -> Option<&str> {
    self.sections.get(section)?.get(key).map(|s| s.as_str())
  }
}

pub struct IniSerializer;

impl IniSerializer {
  pub fn serialize_audio_settings(settings: &AudioSettings) -> String {
    let mut out = String::new();
    out.push_str("[audio]\n");
    let _ = writeln!(out, "single_track = {}", settings.single_track);
    out.push('\n');
    for (i, track) in settings.tracks.iter().enumerate() {
      let _ = writeln!(out, "[track.{}]", i);
      let _ = writeln!(out, "name = {}", track.name);
      let _ = writeln!(out, "index = {}", track.index);
      let _ = writeln!(out, "offset = {:.1}", track.offset);
      out.push('\n');
    }
    out
  }

  pub fn deserialize_audio_settings(content: &str) -> Result<AudioSettings> {
    let doc = IniDocument::parse(content);

    let single_track = doc
      .get("audio", "single_track")
      .map(|v| v == "true")
      .unwrap_or(false);

    let mut tracks = Vec::new();
    let mut i = 0;
    loop {
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

      i += 1;
    }

    Ok(AudioSettings {
      single_track,
      tracks,
    })
  }
}
