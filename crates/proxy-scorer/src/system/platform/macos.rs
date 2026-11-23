use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

use crate::system::{config::ProxyConfig, Endpoint};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";

fn networksetup(args: &[&str]) -> Result<String> {
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
  Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn network_services() -> Result<Vec<String>> {
  let text = networksetup(&["-listallnetworkservices"])?;
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

fn set_proxy(
  service: &str,
  set_flag: &str,
  state_flag: &str,
  endpoint: &Endpoint,
) -> Result<()> {
  let port = endpoint.port.to_string();
  networksetup(&[set_flag, service, &endpoint.host, &port])?;
  networksetup(&[state_flag, service, "on"])?;
  Ok(())
}

fn read_proxy(service: &str, get_flag: &str) -> Result<Option<Endpoint>> {
  let text = networksetup(&[get_flag, service])?;
  if field(&text, "Enabled:").as_deref() != Some("Yes") {
    return Ok(None);
  }
  let (Some(host), Some(port)) =
    (field(&text, "Server:"), field(&text, "Port:"))
  else {
    return Ok(None);
  };
  Ok(Some(Endpoint::new(host, port.parse()?)?))
}

pub fn apply(config: &ProxyConfig) -> Result<()> {
  if config.is_empty() {
    bail!("no proxy endpoints provided");
  }

  for service in network_services()? {
    if let Some(endpoint) = &config.http {
      set_proxy(&service, "-setwebproxy", "-setwebproxystate", endpoint)?;
    }
    if let Some(endpoint) = &config.https {
      set_proxy(
        &service,
        "-setsecurewebproxy",
        "-setsecurewebproxystate",
        endpoint,
      )?;
    }
    if let Some(endpoint) = &config.socks {
      set_proxy(
        &service,
        "-setsocksfirewallproxy",
        "-setsocksfirewallproxystate",
        endpoint,
      )?;
    }
  }
  Ok(())
}

pub fn clear() -> Result<()> {
  for service in network_services()? {
    let _ = networksetup(&["-setwebproxystate", &service, "off"]);
    let _ = networksetup(&["-setsecurewebproxystate", &service, "off"]);
    let _ = networksetup(&["-setsocksfirewallproxystate", &service, "off"]);
  }
  Ok(())
}

pub fn show() -> Result<ProxyConfig> {
  let services = network_services()?;
  let service = services
    .first()
    .ok_or_else(|| anyhow!("no enabled network service found"))?;

  Ok(ProxyConfig {
    http: read_proxy(service, "-getwebproxy")?,
    https: read_proxy(service, "-getsecurewebproxy")?,
    socks: read_proxy(service, "-getsocksfirewallproxy")?,
  })
}
