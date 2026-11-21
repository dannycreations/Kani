use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

use crate::system::{config::ProxyConfig, Endpoint};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";

fn run(args: &[&str]) -> Result<()> {
  let output = Command::new(NETWORKSETUP)
    .args(args)
    .output()
    .with_context(|| format!("failed to run {NETWORKSETUP}"))?;
  if !output.status.success() {
    bail!(
      "networksetup {} failed: {}",
      args.join(" "),
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(())
}

fn output(args: &[&str]) -> Result<String> {
  let result = Command::new(NETWORKSETUP)
    .args(args)
    .output()
    .with_context(|| format!("failed to run {NETWORKSETUP}"))?;
  if !result.status.success() {
    bail!(
      "networksetup {} failed: {}",
      args.join(" "),
      String::from_utf8_lossy(&result.stderr).trim()
    );
  }
  Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}

fn network_services() -> Result<Vec<String>> {
  let text = output(&["-listallnetworkservices"])?;
  Ok(
    text
      .lines()
      .skip(1)
      .map(str::trim)
      .filter(|line| !line.is_empty() && !line.starts_with('*'))
      .map(str::to_owned)
      .collect(),
  )
}

fn field(text: &str, name: &str) -> Option<String> {
  text
    .lines()
    .find_map(|line| line.trim().strip_prefix(name))
    .map(str::trim)
    .map(str::to_owned)
}

pub fn apply(config: &ProxyConfig) -> Result<()> {
  if config.is_empty() {
    bail!("no proxy endpoints provided");
  }

  for service in network_services()? {
    if let Some(endpoint) = &config.http {
      let port = endpoint.port.to_string();
      run(&["-setwebproxy", &service, &endpoint.host, &port])?;
      run(&["-setwebproxystate", &service, "on"])?;
    }
    if let Some(endpoint) = &config.https {
      let port = endpoint.port.to_string();
      run(&["-setsecurewebproxy", &service, &endpoint.host, &port])?;
      run(&["-setsecurewebproxystate", &service, "on"])?;
    }
    if let Some(endpoint) = &config.socks {
      let port = endpoint.port.to_string();
      run(&["-setsocksfirewallproxy", &service, &endpoint.host, &port])?;
      run(&["-setsocksfirewallproxystate", &service, "on"])?;
    }
  }
  Ok(())
}

pub fn clear() -> Result<()> {
  for service in network_services()? {
    let _ = run(&["-setwebproxystate", &service, "off"]);
    let _ = run(&["-setsecurewebproxystate", &service, "off"]);
    let _ = run(&["-setsocksfirewallproxystate", &service, "off"]);
  }
  Ok(())
}

pub fn show() -> Result<ProxyConfig> {
  let services = network_services()?;
  let service = services
    .first()
    .ok_or_else(|| anyhow!("no enabled network service found"))?;

  let mut config = ProxyConfig::default();

  let web = output(&["-getwebproxy", service])?;
  if field(&web, "Enabled:").as_deref() == Some("Yes") {
    if let (Some(host), Some(port)) =
      (field(&web, "Server:"), field(&web, "Port:"))
    {
      config.http = Some(Endpoint::new(host, port.parse()?)?);
    }
  }

  let secure = output(&["-getsecurewebproxy", service])?;
  if field(&secure, "Enabled:").as_deref() == Some("Yes") {
    if let (Some(host), Some(port)) =
      (field(&secure, "Server:"), field(&secure, "Port:"))
    {
      config.https = Some(Endpoint::new(host, port.parse()?)?);
    }
  }

  let socks = output(&["-getsocksfirewallproxy", service])?;
  if field(&socks, "Enabled:").as_deref() == Some("Yes") {
    if let (Some(host), Some(port)) =
      (field(&socks, "Server:"), field(&socks, "Port:"))
    {
      config.socks = Some(Endpoint::new(host, port.parse()?)?);
    }
  }

  Ok(config)
}
