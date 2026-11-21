use std::{fmt, str::FromStr};

use anyhow::{anyhow, bail, Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
  pub host: String,
  pub port: u16,
}

impl Endpoint {
  pub fn new(host: impl Into<String>, port: u16) -> Result<Self> {
    let host = host.into();
    if host.is_empty() {
      bail!("proxy host must not be empty");
    }
    if port == 0 {
      bail!("proxy port must be between 1 and 65535");
    }
    Ok(Self { host, port })
  }

  pub fn socket_address(&self) -> String {
    if self.host.contains(':') && !self.host.starts_with('[') {
      format!("[{}]:{}", self.host, self.port)
    } else {
      format!("{}:{}", self.host, self.port)
    }
  }

  pub fn url(&self, scheme: &str) -> String {
    format!("{}://{}", scheme, self.socket_address())
  }
}

impl FromStr for Endpoint {
  type Err = Error;

  fn from_str(value: &str) -> Result<Self> {
    let value = value.trim().trim_end_matches('/');
    let value = match value.split_once("://") {
      Some((_, rest)) => rest,
      None => value,
    };

    let (host, port) = if let Some(stripped) = value.strip_prefix('[') {
      let (host, rest) = stripped
        .split_once(']')
        .ok_or_else(|| anyhow!("invalid IPv6 proxy endpoint: {value}"))?;
      let port = rest
        .strip_prefix(':')
        .ok_or_else(|| anyhow!("missing port in proxy endpoint: {value}"))?;
      (host, port)
    } else {
      value
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("expected HOST:PORT, got: {value}"))?
    };

    let port = port
      .parse::<u16>()
      .map_err(|_| anyhow!("invalid proxy port: {port}"))?;

    Self::new(host, port)
  }
}

impl fmt::Display for Endpoint {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.socket_address())
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyConfig {
  pub http: Option<Endpoint>,
  pub https: Option<Endpoint>,
  pub socks: Option<Endpoint>,
}

impl ProxyConfig {
  pub fn is_empty(&self) -> bool {
    self.http.is_none() && self.https.is_none() && self.socks.is_none()
  }

  pub fn to_proxy_string(&self) -> String {
    let mut entries = Vec::new();
    if let Some(endpoint) = &self.http {
      entries.push(format!("http={endpoint}"));
    }
    if let Some(endpoint) = &self.https {
      entries.push(format!("https={endpoint}"));
    }
    if let Some(endpoint) = &self.socks {
      entries.push(format!("socks={endpoint}"));
    }
    entries.join(";")
  }

  pub fn from_proxy_string(value: &str) -> Self {
    let mut config = Self::default();
    let value = value.trim();
    if value.is_empty() {
      return config;
    }

    if !value.contains('=') {
      if let Ok(endpoint) = value.parse::<Endpoint>() {
        config.http = Some(endpoint.clone());
        config.https = Some(endpoint);
      }
      return config;
    }

    for entry in value.split(';') {
      let Some((scheme, address)) = entry.split_once('=') else {
        continue;
      };
      let Ok(endpoint) = address.parse::<Endpoint>() else {
        continue;
      };
      match scheme.trim().to_ascii_lowercase().as_str() {
        "http" => config.http = Some(endpoint),
        "https" => config.https = Some(endpoint),
        "socks" | "socks5" | "socks4" => config.socks = Some(endpoint),
        _ => {}
      }
    }
    config
  }

  pub fn to_env_vars(&self) -> Vec<(String, String)> {
    let mut vars = Vec::new();
    if let Some(endpoint) = &self.http {
      let url = endpoint.url("http");
      vars.push(("http_proxy".to_owned(), url.clone()));
      vars.push(("HTTP_PROXY".to_owned(), url));
    }
    if let Some(endpoint) = &self.https {
      let url = endpoint.url("http");
      vars.push(("https_proxy".to_owned(), url.clone()));
      vars.push(("HTTPS_PROXY".to_owned(), url));
    }
    if let Some(endpoint) = &self.socks {
      let url = endpoint.url("socks5");
      vars.push(("all_proxy".to_owned(), url.clone()));
      vars.push(("ALL_PROXY".to_owned(), url));
    }
    vars
  }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;
