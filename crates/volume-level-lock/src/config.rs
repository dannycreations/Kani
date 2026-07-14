#![cfg(windows)]

use std::{
  env, fs,
  path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
#[cfg(test)]
use tempfile::tempdir;

pub struct Config {
  pub input_target: u32,
  pub output_target: u32,
  pub input_paused: bool,
  pub output_paused: bool,
}

impl Config {
  pub fn load() -> Result<Self> {
    Self::load_from_path(&Self::get_path()?)
  }

  pub fn load_from_path(path: &Path) -> Result<Self> {
    let default_config = Self {
      input_target: 100,
      output_target: 100,
      input_paused: false,
      output_paused: false,
    };

    if !path.exists() {
      return Ok(default_config);
    }

    let content = match fs::read_to_string(path) {
      Ok(c) => c,
      Err(_) => {
        let _ = fs::remove_file(path);
        let _ = default_config.save_to_path(path);
        return Ok(default_config);
      }
    };

    let mut input_target = None;
    let mut output_target = None;
    let mut input_paused = None;
    let mut output_paused = None;
    let mut is_corrupted = false;

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
      let Some((key, val)) = line.split_once('=') else {
        is_corrupted = true;
        continue;
      };
      let key = key.trim();
      let val = val.trim();

      match key {
        "input_target" => {
          if let Some(v) =
            val.parse::<u32>().ok().filter(|&v| (1..=100).contains(&v))
          {
            input_target = Some(v);
          } else {
            is_corrupted = true;
          }
        }
        "output_target" => {
          if let Some(v) =
            val.parse::<u32>().ok().filter(|&v| (1..=100).contains(&v))
          {
            output_target = Some(v);
          } else {
            is_corrupted = true;
          }
        }
        "input_paused" => {
          if let Ok(v) = val.parse::<bool>() {
            input_paused = Some(v);
          } else {
            is_corrupted = true;
          }
        }
        "output_paused" => {
          if let Ok(v) = val.parse::<bool>() {
            output_paused = Some(v);
          } else {
            is_corrupted = true;
          }
        }
        _ => {
          is_corrupted = true;
        }
      }
    }

    match (
      is_corrupted,
      input_target,
      output_target,
      input_paused,
      output_paused,
    ) {
      (false, Some(in_t), Some(out_t), Some(in_p), Some(out_p)) => Ok(Self {
        input_target: in_t,
        output_target: out_t,
        input_paused: in_p,
        output_paused: out_p,
      }),
      _ => {
        let _ = fs::remove_file(path);
        let _ = default_config.save_to_path(path);
        Ok(default_config)
      }
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
    let local_app_data = env::var("LOCALAPPDATA")
      .map(PathBuf::from)
      .or_else(|_| {
        env::var("USERPROFILE")
          .map(|p| PathBuf::from(p).join("AppData").join("Local"))
      })
      .map_err(|_| anyhow!("Could not determine local app data directory"))?;
    Ok(local_app_data.join("VolumeLevelLock").join("config.txt"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_load_non_existent() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.txt");
    let cfg = Config::load_from_path(&file_path).unwrap();
    assert_eq!(cfg.input_target, 100);
    assert_eq!(cfg.output_target, 100);
    assert!(!cfg.input_paused);
    assert!(!cfg.output_paused);
  }

  #[test]
  fn test_load_key_value() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.txt");
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
    let file_path = temp_dir.path().join("config.txt");
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
    let file_path = temp_dir.path().join("config.txt");
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
}
