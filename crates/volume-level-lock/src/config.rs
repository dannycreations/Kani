#![cfg(windows)]

use std::{
  env, fs,
  io::ErrorKind,
  path::{Path, PathBuf},
  sync::atomic::Ordering,
};

use anyhow::{anyhow, Result};

use crate::enforcer::VolumeState;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

#[derive(Debug, PartialEq)]
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
    let content = match fs::read_to_string(path) {
      Ok(content) => content,
      // A missing file is left missing; only a present but unusable one
      // gets rewritten.
      Err(err) if err.kind() == ErrorKind::NotFound => {
        return Ok(Self::default());
      }
      // An unreadable file is treated the same as a corrupted one, so
      // both take the same rewrite path.
      Err(_) => return Ok(Self::reset_to_default(path)),
    };

    Ok(Self::parse(&content).unwrap_or_else(|| Self::reset_to_default(path)))
  }

  fn parse(content: &str) -> Option<Self> {
    let mut input_target = None;
    let mut output_target = None;
    let mut input_paused = None;
    let mut output_paused = None;

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
      let (key, val) = line.split_once('=')?;
      match (key.trim(), val.trim()) {
        ("input_target", v) => input_target = Some(parse_percent(v)?),
        ("output_target", v) => output_target = Some(parse_percent(v)?),
        ("input_paused", v) => input_paused = Some(v.parse().ok()?),
        ("output_paused", v) => output_paused = Some(v.parse().ok()?),
        _ => return None,
      }
    }

    Some(Self {
      input_target: input_target?,
      output_target: output_target?,
      input_paused: input_paused?,
      output_paused: output_paused?,
    })
  }

  fn reset_to_default(path: &Path) -> Self {
    let default_config = Self::default();
    // `save_to_path` truncates, so no separate delete is needed.
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

  pub fn apply_to(&self, state: &VolumeState) -> bool {
    if *self == Self::from_state(state) {
      return false;
    }
    state
      .input_target
      .store(self.input_target, Ordering::SeqCst);
    state
      .output_target
      .store(self.output_target, Ordering::SeqCst);
    state
      .input_paused
      .store(self.input_paused, Ordering::SeqCst);
    state
      .output_paused
      .store(self.output_paused, Ordering::SeqCst);
    true
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
