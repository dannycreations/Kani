use std::{fmt::Write as _, path::Path};

use rustc_hash::FxHashSet;

const DEFAULT_POLLING_INTERVAL_MS: u64 = 2000;
const DEFAULT_MIN_WINDOW_SIZE: i32 = 800;

#[derive(Debug, Clone)]
pub struct Config {
  pub process_names: Vec<String>,
  pub polling_interval_ms: u64,
  pub min_window_size: i32,
  monitored_lookup: FxHashSet<Box<str>>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      process_names: Vec::new(),
      polling_interval_ms: DEFAULT_POLLING_INTERVAL_MS,
      min_window_size: DEFAULT_MIN_WINDOW_SIZE,
      monitored_lookup: FxHashSet::default(),
    }
  }
}

impl Config {
  pub fn load_or_create(path: &Path) -> Self {
    match std::fs::read_to_string(path) {
      Ok(content) => Self::parse_ini(&content),
      Err(_) => {
        let default_cfg = Self::default();
        let _ = std::fs::write(path, default_cfg.serialize_ini());
        default_cfg
      }
    }
  }

  fn parse_ini(content: &str) -> Self {
    let mut cfg = Self::default();

    for line in content.lines() {
      let line = line.trim();
      if line.is_empty()
        || line.starts_with('[')
        || line.starts_with(';')
        || line.starts_with('#')
      {
        continue;
      }

      let Some((key, value)) = line.split_once('=') else {
        continue;
      };

      match key.trim() {
        "process_names" => {
          cfg.process_names = value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
              if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
                &s[..s.len() - 4]
              } else {
                s
              }
              .to_owned()
            })
            .collect();
        }
        "polling_interval_ms" => {
          if let Ok(v) = value.trim().parse() {
            cfg.polling_interval_ms = v;
          }
        }
        "min_window_size" => {
          if let Ok(v) = value.trim().parse() {
            cfg.min_window_size = v;
          }
        }
        _ => {}
      }
    }

    cfg.rebuild_lookup();
    cfg
  }

  pub fn rebuild_lookup(&mut self) {
    self.monitored_lookup.clear();
    for name in &self.process_names {
      self
        .monitored_lookup
        .insert(name.to_ascii_lowercase().into_boxed_str());
    }
  }

  #[inline]
  pub fn is_monitored(&self, name: &str) -> bool {
    if self.monitored_lookup.is_empty() {
      return false;
    }

    if name.len() <= 64 {
      let mut buf = [0u8; 64];
      let bytes = name.as_bytes();
      for i in 0..bytes.len() {
        buf[i] = bytes[i].to_ascii_lowercase();
      }
      if let Ok(lower) = std::str::from_utf8(&buf[..bytes.len()]) {
        return self.monitored_lookup.contains(lower);
      }
    }

    self
      .monitored_lookup
      .contains(name.to_ascii_lowercase().as_str())
  }

  pub fn add_process(&mut self, name: String) -> bool {
    if self.is_monitored(&name) {
      return false;
    }
    self
      .monitored_lookup
      .insert(name.to_ascii_lowercase().into_boxed_str());
    self.process_names.push(name);
    true
  }

  pub fn remove_process(&mut self, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    self.monitored_lookup.remove(lower.as_str());
    let prev_len = self.process_names.len();
    self.process_names.retain(|n| !n.eq_ignore_ascii_case(name));
    self.process_names.len() != prev_len
  }

  pub fn clear_processes(&mut self) {
    self.process_names.clear();
    self.monitored_lookup.clear();
  }

  fn serialize_ini(&self) -> String {
    let mut out = String::with_capacity(128);
    out.push_str("[settings]\nprocess_names = ");
    for (i, name) in self.process_names.iter().enumerate() {
      if i > 0 {
        out.push_str(", ");
      }
      out.push_str(name);
    }
    let _ = writeln!(
      out,
      "\npolling_interval_ms = {}\nmin_window_size = {}",
      self.polling_interval_ms, self.min_window_size
    );
    out
  }

  pub fn save(&self, path: &Path) -> std::io::Result<()> {
    std::fs::write(path, self.serialize_ini())
  }
}
