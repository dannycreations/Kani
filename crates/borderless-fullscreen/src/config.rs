use std::{fmt::Write as _, io::ErrorKind, path::Path};

use crate::clog;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

const DEFAULT_POLLING_INTERVAL_MS: u64 = 2000;
const DEFAULT_MIN_WINDOW_SIZE: i32 = 800;

const EXE_SUFFIX: &[u8] = b".exe";

#[inline]
fn strip_exe_suffix(s: &str) -> &str {
  // Compared as bytes: a name whose last four bytes fall inside a
  // multi-byte character has no `&str` slice at that offset, and this runs
  // on every enumerated process name.
  let tail = &s.as_bytes()[s.len().saturating_sub(EXE_SUFFIX.len())..];
  if tail.eq_ignore_ascii_case(EXE_SUFFIX) {
    &s[..s.len() - EXE_SUFFIX.len()]
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
    let mut out = String::from("[settings]\nprocess_names = ");
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
