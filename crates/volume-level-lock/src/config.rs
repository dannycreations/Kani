#![cfg(windows)]

use std::{
  env, fs,
  path::{Path, PathBuf},
  sync::atomic::Ordering,
};

use anyhow::{anyhow, Result};
#[cfg(test)]
use tempfile::tempdir;

use crate::enforcer::VolumeState;

pub struct Config {
  pub input_target: u32,
  pub output_target: u32,
  pub input_paused: bool,
  pub output_paused: bool,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      input_target: 100,
      output_target: 100,
      input_paused: false,
      output_paused: false,
    }
  }
}

fn parse_percent(val: &str) -> Option<u32> {
  val.parse::<u32>().ok().filter(|&v| (1..=100).contains(&v))
}

impl Config {
  pub fn load() -> Result<Self> {
    Self::load_from_path(&Self::get_path()?)
  }

  pub fn load_from_path(path: &Path) -> Result<Self> {
    if !path.exists() {
      return Ok(Self::default());
    }

    // Unreadable file is treated the same as corrupted,
    // avoiding a separate error branch further down.
    let Ok(content) = fs::read_to_string(path) else {
      return Ok(Self::reset_to_default(path));
    };

    let mut input_target = None;
    let mut output_target = None;
    let mut input_paused = None;
    let mut output_paused = None;
    let mut corrupted = false;

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
      let Some((key, val)) = line.split_once('=') else {
        corrupted = true;
        continue;
      };
      let (key, val) = (key.trim(), val.trim());

      match key {
        "input_target" => match parse_percent(val) {
          Some(v) => input_target = Some(v),
          None => corrupted = true,
        },
        "output_target" => match parse_percent(val) {
          Some(v) => output_target = Some(v),
          None => corrupted = true,
        },
        "input_paused" => match val.parse::<bool>() {
          Ok(v) => input_paused = Some(v),
          Err(_) => corrupted = true,
        },
        "output_paused" => match val.parse::<bool>() {
          Ok(v) => output_paused = Some(v),
          Err(_) => corrupted = true,
        },
        _ => corrupted = true,
      }
    }

    // A missing field is equivalent to corruption: both paths reset to
    // defaults, so the two checks are unified into one bail-out point.
    let (
      Some(input_target),
      Some(output_target),
      Some(input_paused),
      Some(output_paused),
    ) = (input_target, output_target, input_paused, output_paused)
    else {
      return Ok(Self::reset_to_default(path));
    };

    if corrupted {
      return Ok(Self::reset_to_default(path));
    }

    Ok(Self {
      input_target,
      output_target,
      input_paused,
      output_paused,
    })
  }

  fn reset_to_default(path: &Path) -> Self {
    let default_config = Self::default();
    let _ = fs::remove_file(path);
    let _ = default_config.save_to_path(path);
    default_config
  }

  pub fn from_state(state: &VolumeState) -> Self {
    Self {
      input_target: state.input_target.load(Ordering::SeqCst),
      output_target: state.output_target.load(Ordering::SeqCst),
      input_paused: state.input_paused.load(Ordering::SeqCst),
      output_paused: state.output_paused.load(Ordering::SeqCst),
    }
  }

  pub fn save(&self) -> Result<()> {
    self.save_to_path(&Self::get_path()?)
  }

  pub fn save_to_path(&self, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let content = format!(
      "input_target={}\noutput_target={}\ninput_paused={}\noutput_paused={}\n",
      self.input_target,
      self.output_target,
      self.input_paused,
      self.output_paused
    );
    fs::write(path, content)?;
    Ok(())
  }

  pub fn get_path() -> Result<PathBuf> {
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path
      .parent()
      .ok_or_else(|| anyhow!("Could not determine executable directory"))?;
    let stem = exe_path
      .file_stem()
      .ok_or_else(|| anyhow!("Could not determine executable name"))?;
    Ok(exe_dir.join(stem).with_extension("ini"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_load_non_existent() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.ini");
    let cfg = Config::load_from_path(&file_path).unwrap();
    assert_eq!(cfg.input_target, 100);
    assert_eq!(cfg.output_target, 100);
    assert!(!cfg.input_paused);
    assert!(!cfg.output_paused);
  }

  #[test]
  fn test_load_key_value() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.ini");
    fs::write(
      &file_path,
      "input_target=45\noutput_target=90\ninput_paused=true\noutput_paused=false\n",
    )
    .unwrap();
    let cfg = Config::load_from_path(&file_path).unwrap();
    assert_eq!(cfg.input_target, 45);
    assert_eq!(cfg.output_target, 90);
    assert!(cfg.input_paused);
    assert!(!cfg.output_paused);
  }

  #[test]
  fn test_save_and_load() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.ini");
    let cfg = Config {
      input_target: 30,
      output_target: 80,
      input_paused: false,
      output_paused: true,
    };
    cfg.save_to_path(&file_path).unwrap();
    let loaded = Config::load_from_path(&file_path).unwrap();
    assert_eq!(loaded.input_target, 30);
    assert_eq!(loaded.output_target, 80);
    assert!(!loaded.input_paused);
    assert!(loaded.output_paused);
  }

  #[test]
  fn test_corrupted_config_rewritten() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.ini");
    fs::write(&file_path, "invalid_junk_data_here").unwrap();

    // Loading should detect corruption, delete the file, write defaults, and return default
    let cfg = Config::load_from_path(&file_path).unwrap();
    assert_eq!(cfg.input_target, 100);
    assert_eq!(cfg.output_target, 100);
    assert!(!cfg.input_paused);
    assert!(!cfg.output_paused);

    // Verify the file was indeed replaced with defaults
    let replaced_content = fs::read_to_string(&file_path).unwrap();
    assert!(replaced_content.contains("input_target=100"));
    assert!(replaced_content.contains("output_target=100"));
    assert!(replaced_content.contains("input_paused=false"));
    assert!(replaced_content.contains("output_paused=false"));
  }

  #[test]
  fn test_from_state_snapshot() {
    let state = VolumeState::new(42, 77, true, false);
    let cfg = Config::from_state(&state);
    assert_eq!(cfg.input_target, 42);
    assert_eq!(cfg.output_target, 77);
    assert!(cfg.input_paused);
    assert!(!cfg.output_paused);
  }
}
