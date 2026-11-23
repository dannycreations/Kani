use std::{collections::HashSet, time::Duration};

use anyhow::{Context, Result};
use reqwest::blocking::Client;

use crate::model::Proxy;

pub const DEFAULT_SOURCE: &str =
  "https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/all/data.txt";

pub fn fetch(url: &str, timeout: Duration) -> Result<Vec<Proxy>> {
  let client = Client::builder().timeout(timeout).build()?;
  let response = client
    .get(url)
    .send()
    .with_context(|| format!("failed to fetch proxy list from {url}"))?
    .error_for_status()
    .with_context(|| format!("proxy list request failed for {url}"))?;
  let body = response.text().context("failed to read proxy list body")?;
  Ok(parse_list(&body))
}

pub fn parse_list(text: &str) -> Vec<Proxy> {
  let mut seen = HashSet::new();
  let mut proxies = Vec::new();
  for proxy in text.lines().filter_map(Proxy::parse) {
    if !seen.contains(&proxy) {
      seen.insert(proxy.clone());
      proxies.push(proxy);
    }
  }
  proxies
}

#[cfg(test)]
#[path = "fetch_test.rs"]
mod fetch_test;
