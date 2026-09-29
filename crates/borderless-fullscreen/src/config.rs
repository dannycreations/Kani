use std::{fmt::Write as _, io::ErrorKind, path::Path};

use crate::clog;

const DEFAULT_POLLING_INTERVAL_MS: u64 = 2000;
const DEFAULT_MIN_WINDOW_SIZE: i32 = 800;

#[inline]
fn strip_exe_suffix(s: &str) -> &str {
  if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
    &s[..s.len() - 4]
  } else {
    s
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Config {
  pub process_names: Vec<String>,
  pub polling_interval_ms: u64,
  pub min_window_size: i32,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      process_names: Vec::new(),
      polling_interval_ms: DEFAULT_POLLING_INTERVAL_MS,
      min_window_size: DEFAULT_MIN_WINDOW_SIZE,
    }
  }
}

impl Config {
  pub fn load_or_create(path: &Path) -> Self {
    match std::fs::read_to_string(path) {
      Ok(content) => Self::parse_ini(&content),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        let default_cfg = Self::default();
        if let Err(err) = default_cfg.save(path) {
          clog!("Failed to write default config {}: {err}", path.display());
        }
        default_cfg
      }
      Err(err) => {
        clog!("Failed to read config {}: {err}", path.display());
        Self::default()
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

    cfg
  }

  #[inline]
  pub fn is_monitored(&self, name: &str) -> bool {
    let name = strip_exe_suffix(name);
    self
      .process_names
      .iter()
      .any(|n| n.eq_ignore_ascii_case(name))
  }

  pub fn add_process(&mut self, name: &str) -> bool {
    if self.is_monitored(name) {
      return false;
    }
    self.process_names.push(strip_exe_suffix(name).to_owned());
    true
  }

  pub fn remove_process(&mut self, name: &str) -> bool {
    let name = strip_exe_suffix(name);
    let prev_len = self.process_names.len();
    self.process_names.retain(|n| !n.eq_ignore_ascii_case(name));
    self.process_names.len() != prev_len
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
    assert!(!config.add_process("GAME"), "duplicate ignored");

    assert!(config.add_process("notes.exe"));
    assert_eq!(config.process_names, vec!["game", "notes"]);
    assert!(config.is_monitored("NOTES.exe"));

    assert!(config.remove_process("game.exe"));
    assert!(!config.is_monitored("Game"));
    assert!(!config.remove_process("game"), "already absent");
  }

  #[test]
  fn load_or_create_seeds_a_missing_file_and_keeps_an_existing_one() {
    let path = std::env::temp_dir().join("borderless_fullscreen_cfg_test.ini");
    let _ = std::fs::remove_file(&path);

    let seeded = Config::load_or_create(&path);
    assert_eq!(seeded, Config::default());
    assert_eq!(
      std::fs::read_to_string(&path).unwrap(),
      seeded.serialize_ini()
    );

    let mut edited = seeded;
    edited.add_process("game.exe");
    edited.save(&path).unwrap();

    let reloaded = Config::load_or_create(&path);
    assert!(reloaded.is_monitored("game"), "reloaded keeps the process");
    assert_eq!(reloaded.polling_interval_ms, edited.polling_interval_ms);

    std::fs::remove_file(&path).unwrap();
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
