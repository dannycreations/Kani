#![cfg(windows)]

use std::{env, fs, path::PathBuf};

use anyhow::{anyhow, Result};

pub struct Config {
  pub target_percent: u32,
}

impl Config {
  pub fn load() -> Result<Self> {
    let path = Self::get_path()?;
    if !path.exists() {
      return Ok(Self {
        target_percent: 100,
      });
    }
    let content = fs::read_to_string(path)?;
    let target_percent =
      content.trim().parse::<u32>().unwrap_or(100).clamp(1, 100);
    Ok(Self { target_percent })
  }

  pub fn save(&self) -> Result<()> {
    let path = Self::get_path()?;
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    fs::write(path, self.target_percent.to_string())?;
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
    Ok(local_app_data.join("VolLevelLock").join("config.txt"))
  }
}
