use std::{env, fs, io::ErrorKind, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};

use crate::system::{config::ProxyConfig, Endpoint};

const ENV_FILE: &str = "90-kani-proxy.conf";

fn env_file() -> Result<PathBuf> {
  let home = env::var("HOME").context("HOME is not set")?;
  let directory = PathBuf::from(home).join(".config").join("environment.d");
  fs::create_dir_all(&directory).context("failed to create environment.d")?;
  Ok(directory.join(ENV_FILE))
}

fn gsettings(schema: &str, key: &str, value: &str) -> Result<()> {
  Command::new("gsettings")
    .args(["set", schema, key, value])
    .status()
    .with_context(|| format!("failed to set {schema} {key}"))?;
  Ok(())
}

pub fn apply(config: &ProxyConfig) -> Result<()> {
  if config.is_empty() {
    bail!("no proxy endpoints provided");
  }

  if let Some(endpoint) = &config.http {
    let _ = gsettings("org.gnome.system.proxy.http", "host", &endpoint.host);
    let _ = gsettings(
      "org.gnome.system.proxy.http",
      "port",
      &endpoint.port.to_string(),
    );
  }
  if let Some(endpoint) = &config.https {
    let _ = gsettings("org.gnome.system.proxy.https", "host", &endpoint.host);
    let _ = gsettings(
      "org.gnome.system.proxy.https",
      "port",
      &endpoint.port.to_string(),
    );
  }
  if let Some(endpoint) = &config.socks {
    let _ = gsettings("org.gnome.system.proxy.socks", "host", &endpoint.host);
    let _ = gsettings(
      "org.gnome.system.proxy.socks",
      "port",
      &endpoint.port.to_string(),
    );
  }
  let _ = gsettings("org.gnome.system.proxy", "mode", "manual");

  let mut content = String::new();
  for (key, value) in config.to_env_vars() {
    content.push_str(&format!("{key}={value}\n"));
  }
  fs::write(env_file()?, content)
    .context("failed to write proxy environment file")?;
  Ok(())
}

pub fn clear() -> Result<()> {
  let _ = gsettings("org.gnome.system.proxy", "mode", "none");
  let path = env_file()?;
  if path.exists() {
    fs::remove_file(path).context("failed to remove proxy environment file")?;
  }
  Ok(())
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
