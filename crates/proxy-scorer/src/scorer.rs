use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::{
  model::{Proxy, Score, ScoredProxy},
  probe::{self, ProbeConfig},
};

#[derive(Clone, Debug)]
pub struct ScoringConfig {
  pub workers: Option<usize>,
  pub tries: u32,
  pub connect_timeout: Duration,
  pub timeout: Duration,
  pub test_url: String,
  pub success_weight: f64,
  pub latency_weight: f64,
  pub latency_reference_ms: f64,
  pub show_progress: bool,
}

impl Default for ScoringConfig {
  fn default() -> Self {
    Self {
      workers: None,
      tries: 2,
      connect_timeout: Duration::from_millis(3_000),
      timeout: Duration::from_millis(8_000),
      test_url: "https://example.com".to_owned(),
      success_weight: 0.7,
      latency_weight: 0.3,
      latency_reference_ms: 1_000.0,
      show_progress: true,
    }
  }
}

pub fn score_one(proxy: &Proxy, config: &ScoringConfig) -> ScoredProxy {
  let probe_config = ProbeConfig {
    connect_timeout: config.connect_timeout,
    timeout: config.timeout,
    test_url: config.test_url.clone(),
  };

  let Ok(client) = probe::build_client(proxy, &probe_config) else {
    return ScoredProxy {
      proxy: proxy.clone(),
      score: Score::default(),
    };
  };

  let mut successes = 0u32;
  let mut total_latency = 0u64;
  for _ in 0..config.tries {
    if let Ok(latency) = probe::probe(&client, &probe_config) {
      successes += 1;
      total_latency += latency;
    }
  }

  let attempts = config.tries;
  let success_rate = if attempts == 0 {
    0.0
  } else {
    f64::from(successes) / f64::from(attempts)
  };
  let latency_ms = if successes == 0 {
    0
  } else {
    total_latency / u64::from(successes)
  };

  let weight_sum = config.success_weight + config.latency_weight;
  let value = if successes == 0 || weight_sum <= 0.0 {
    0.0
  } else {
    let latency_factor =
      (-(latency_ms as f64) / config.latency_reference_ms).exp();
    100.0
      * (config.success_weight * success_rate
        + config.latency_weight * latency_factor)
      / weight_sum
  };

  ScoredProxy {
    proxy: proxy.clone(),
    score: Score {
      value,
      success_rate,
      latency_ms,
      successes,
      attempts,
    },
  }
}

fn make_progress(total: usize, enabled: bool) -> Option<ProgressBar> {
  if !enabled {
    return None;
  }
  let bar = ProgressBar::new(total as u64);
  let style = ProgressStyle::with_template(
    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} proxies",
  )
  .expect("valid progress template");
  bar.set_style(style);
  bar.enable_steady_tick(Duration::from_millis(100));
  Some(bar)
}

pub fn score_all(
  proxies: Vec<Proxy>,
  config: &ScoringConfig,
) -> Vec<ScoredProxy> {
  let progress = make_progress(proxies.len(), config.show_progress);

  let score_fn = |proxy: &Proxy| {
    let result = score_one(proxy, config);
    if let Some(bar) = &progress {
      bar.inc(1);
    }
    result
  };

  let mut scored: Vec<ScoredProxy> = match config.workers {
    Some(workers) => {
      let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .expect("valid thread pool size");
      pool.install(|| proxies.par_iter().map(score_fn).collect())
    }
    None => proxies.par_iter().map(score_fn).collect(),
  };

  if let Some(bar) = &progress {
    bar.finish_and_clear();
  }

  scored.sort_by(|a, b| {
    b.score
      .value
      .total_cmp(&a.score.value)
      .then_with(|| a.score.latency_ms.cmp(&b.score.latency_ms))
  });
  scored
}

#[cfg(test)]
#[path = "scorer_test.rs"]
mod scorer_test;
