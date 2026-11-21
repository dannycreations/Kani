use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::model::ScoredProxy;

#[derive(Serialize)]
struct Report<'a> {
  total: usize,
  results: &'a [ScoredProxy],
}

pub fn to_json(scored: &[ScoredProxy]) -> Result<String> {
  serde_json::to_string_pretty(&Report {
    total: scored.len(),
    results: scored,
  })
  .context("failed to serialize results")
}

pub fn print_table(scored: &[ScoredProxy]) {
  println!(
    "{:>4}  {:>7}  {:<9}  {:>22}  {:>7}  {:>8}",
    "#", "SCORE", "SCHEME", "PROXY", "LATENCY", "SUCCESS"
  );
  for (index, item) in scored.iter().enumerate() {
    println!(
      "{:>4}  {:>7.2}  {:<9}  {:>22}  {:>6}ms  {:>3}/{:<4}",
      index + 1,
      item.score.value,
      item.proxy.scheme,
      item.proxy.endpoint(),
      item.score.latency_ms,
      item.score.successes,
      item.score.attempts,
    );
  }
}

pub fn write_file(path: &Path, contents: &str) -> Result<()> {
  fs::write(path, contents)
    .with_context(|| format!("failed to write {}", path.display()))
}
