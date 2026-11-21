use std::{fmt, str::FromStr};

use anyhow::{anyhow, Error, Result};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
  Http,
  Https,
  Socks4,
  Socks5,
}

impl Scheme {
  pub const ALL: [Scheme; 4] =
    [Scheme::Http, Scheme::Https, Scheme::Socks4, Scheme::Socks5];

  pub fn as_str(self) -> &'static str {
    match self {
      Scheme::Http => "http",
      Scheme::Https => "https",
      Scheme::Socks4 => "socks4",
      Scheme::Socks5 => "socks5",
    }
  }

  pub fn is_socks(self) -> bool {
    matches!(self, Scheme::Socks4 | Scheme::Socks5)
  }
}

impl FromStr for Scheme {
  type Err = Error;

  fn from_str(value: &str) -> Result<Self> {
    match value.trim().to_ascii_lowercase().as_str() {
      "http" => Ok(Scheme::Http),
      "https" => Ok(Scheme::Https),
      "socks4" => Ok(Scheme::Socks4),
      "socks5" => Ok(Scheme::Socks5),
      other => Err(anyhow!("unknown proxy scheme: {other}")),
    }
  }
}

impl fmt::Display for Scheme {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Proxy {
  pub scheme: Scheme,
  pub host: String,
  pub port: u16,
}

impl Proxy {
  pub fn parse(line: &str) -> Option<Self> {
    let line = line.trim();
    let (scheme, address) = line.split_once("://")?;
    let scheme = scheme.parse().ok()?;

    let (host, port) = if let Some(stripped) = address.strip_prefix('[') {
      let (host, rest) = stripped.split_once(']')?;
      (host, rest.strip_prefix(':')?)
    } else {
      address.rsplit_once(':')?
    };

    let port = port.parse().ok()?;
    if host.is_empty() || port == 0 {
      return None;
    }

    Some(Self {
      scheme,
      host: host.to_owned(),
      port,
    })
  }

  pub fn url(&self) -> String {
    let host = if self.host.contains(':') && !self.host.starts_with('[') {
      format!("[{}]", self.host)
    } else {
      self.host.clone()
    };
    format!("{}://{}:{}", self.scheme, host, self.port)
  }

  pub fn endpoint(&self) -> String {
    let host = if self.host.contains(':') && !self.host.starts_with('[') {
      format!("[{}]", self.host)
    } else {
      self.host.clone()
    };
    format!("{host}:{}", self.port)
  }
}

impl fmt::Display for Proxy {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.url())
  }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Score {
  pub value: f64,
  pub success_rate: f64,
  pub latency_ms: u64,
  pub successes: u32,
  pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScoredProxy {
  pub proxy: Proxy,
  pub score: Score,
}

#[cfg(test)]
#[path = "model_test.rs"]
mod model_test;
