use std::{fmt::Write as _, path::Path};

use rustc_hash::FxHashSet;

const DEFAULT_POLLING_INTERVAL_MS: u64 = 2000;
const DEFAULT_MIN_WINDOW_SIZE: i32 = 800;

#[inline]
pub fn strip_exe_suffix(s: &str) -> &str {
  if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
    &s[..s.len() - 4]
  } else {
    s
  }
}

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
            .map(strip_exe_suffix)
            .map(ToOwned::to_owned)
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

  fn rebuild_lookup(&mut self) {
    self.monitored_lookup.clear();
    // Pre-size the derived set once per rebuild instead of letting it
    // grow and rehash incrementally as names are pushed.
    self.monitored_lookup.reserve(self.process_names.len());
    for name in &self.process_names {
      self
        .monitored_lookup
        .insert(strip_exe_suffix(name).to_ascii_lowercase().into());
    }
  }

  #[inline]
  pub fn is_monitored(&self, name: &str) -> bool {
    if self.monitored_lookup.is_empty() {
      return false;
    }
    self
      .monitored_lookup
      .contains(strip_exe_suffix(name).to_ascii_lowercase().as_str())
  }

  pub fn add_process(&mut self, name: String) -> bool {
    let stripped = strip_exe_suffix(&name);
    if self.is_monitored(stripped) {
      return false;
    }
    self.process_names.push(stripped.to_owned());
    self.rebuild_lookup();
    true
  }

  pub fn remove_process(&mut self, name: &str) -> bool {
    let stripped = strip_exe_suffix(name);
    let prev_len = self.process_names.len();
    self
      .process_names
      .retain(|n| !n.eq_ignore_ascii_case(stripped));
    if self.process_names.len() != prev_len {
      self.rebuild_lookup();
      true
    } else {
      false
    }
  }

  pub fn clear_processes(&mut self) {
    self.process_names.clear();
    self.monitored_lookup.clear();
  }

  fn serialize_ini(&self) -> String {
    let mut out = String::with_capacity(64 + self.process_names.len() * 20);
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

#[cfg(test)]
mod tests {
  use super::*;

  fn config_with(process_names: &str) -> Config {
    Config::parse_ini(&format!("[settings]\nprocess_names = {process_names}\n"))
  }

  #[test]
  fn matching_ignores_case_and_exe_suffix() {
    let config = config_with("Game.exe, notepad");

    for name in ["Game", "game", "GAME.EXE", "game.Exe", "Notepad"] {
      assert!(config.is_monitored(name), "{name} should be monitored");
    }
    for name in ["gam", "games", "notepad2", "ote"] {
      assert!(!config.is_monitored(name), "{name} should not match");
    }
  }

  #[test]
  fn add_and_remove_keep_the_lookup_in_sync() {
    let mut config = config_with("game");
    assert!(!config.add_process("GAME".to_owned()), "duplicate ignored");

    assert!(config.add_process("notes.exe".to_owned()));
    assert!(config.is_monitored("NOTES.exe"));

    assert!(config.remove_process("game.exe"));
    assert!(!config.is_monitored("Game"));
    assert!(!config.remove_process("game"), "already absent");
  }

  #[test]
  fn ini_round_trip_preserves_matching() {
    let mut config = config_with("Game.exe");
    config.polling_interval_ms = 5000;
    config.min_window_size = 640;

    let reparsed = Config::parse_ini(&config.serialize_ini());

    assert_eq!(reparsed.process_names, vec!["Game".to_owned()]);
    assert_eq!(reparsed.polling_interval_ms, 5000);
    assert_eq!(reparsed.min_window_size, 640);
    assert!(reparsed.is_monitored("game"));
  }
}
