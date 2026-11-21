use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use reqwest::{blocking::Client, Proxy as HttpProxy};

use crate::model::Proxy;

#[derive(Clone, Debug)]
pub struct ProbeConfig {
  pub connect_timeout: Duration,
  pub timeout: Duration,
  pub test_url: String,
}

pub fn build_client(proxy: &Proxy, config: &ProbeConfig) -> Result<Client> {
  let proxy = HttpProxy::all(proxy.url())
    .with_context(|| format!("invalid proxy URL: {proxy}"))?;
  Client::builder()
    .proxy(proxy)
    .connect_timeout(config.connect_timeout)
    .timeout(config.timeout)
    .build()
    .context("failed to build probe client")
}

pub fn probe(client: &Client, config: &ProbeConfig) -> Result<u64> {
  let started = Instant::now();
  let response = client
    .get(&config.test_url)
    .send()
    .context("proxy request failed")?;
  let latency_ms = started.elapsed().as_millis() as u64;

  let status = response.status();
  if !status.is_success() {
    bail!("proxy returned HTTP {status}");
  }
  Ok(latency_ms)
}
