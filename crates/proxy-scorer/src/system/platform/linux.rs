use std::{env, fs, io::ErrorKind, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};

use crate::system::{config::ProxyConfig, Endpoint};

const ENV_FILE: &str = "90-kani-proxy.conf";

fn env_file() -> Result<PathBuf> {
  let home = env::var("HOME").context("HOME is not set")?;
  Ok(
    PathBuf::from(home)
      .join(".config")
      .join("environment.d")
      .join(ENV_FILE),
  )
}

fn gsettings(schema: &str, key: &str, value: &str) {
  let _ = Command::new("gsettings")
    .args(["set", schema, key, value])
    .status();
}

fn set_gnome_proxy(protocol: &str, endpoint: &Endpoint) {
  let schema = format!("org.gnome.system.proxy.{protocol}");
  gsettings(&schema, "host", &endpoint.host);
  gsettings(&schema, "port", &endpoint.port.to_string());
}

pub fn apply(config: &ProxyConfig) -> Result<()> {
  if config.is_empty() {
    bail!("no proxy endpoints provided");
  }

  if let Some(endpoint) = &config.http {
    set_gnome_proxy("http", endpoint);
  }
  if let Some(endpoint) = &config.https {
    set_gnome_proxy("https", endpoint);
  }
  if let Some(endpoint) = &config.socks {
    set_gnome_proxy("socks", endpoint);
  }
  gsettings("org.gnome.system.proxy", "mode", "manual");

  let path = env_file()?;
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).context("failed to create environment.d")?;
  }

  let mut content = String::new();
  for (key, value) in config.to_env_vars() {
    content.push_str(&format!("{key}={value}\n"));
  }
  fs::write(path, content).context("failed to write proxy environment file")?;
  Ok(())
}

pub fn clear() -> Result<()> {
  gsettings("org.gnome.system.proxy", "mode", "none");
  let path = env_file()?;
  match fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error).context("failed to remove proxy environment file"),
  }
}

pub fn show() -> Result<ProxyConfig> {
  let path = env_file()?;
  let content = match fs::read_to_string(path) {
    Ok(content) => content,
    Err(error) if error.kind() == ErrorKind::NotFound => {
      return Ok(ProxyConfig::default());
    }
    Err(error) => return Err(error.into()),
  };

  let mut config = ProxyConfig::default();
  for line in content.lines() {
    let Some((key, value)) = line.split_once('=') else {
      continue;
    };
    let Ok(endpoint) = value.parse::<Endpoint>() else {
      continue;
    };
    match key {
      "http_proxy" | "HTTP_PROXY" => config.http = Some(endpoint),
      "https_proxy" | "HTTPS_PROXY" => config.https = Some(endpoint),
      "all_proxy" | "ALL_PROXY" => config.socks = Some(endpoint),
      _ => {}
    }
  }
  Ok(config)
}
