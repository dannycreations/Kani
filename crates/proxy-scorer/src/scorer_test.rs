use std::time::Duration;

use super::*;
use crate::model::Scheme;

fn dead_proxy() -> Proxy {
  Proxy {
    scheme: Scheme::Http,
    host: "127.0.0.1".to_owned(),
    port: 1,
  }
}

#[test]
fn dead_proxy_scores_zero() {
  let config = ScoringConfig {
    tries: 1,
    connect_timeout: Duration::from_millis(100),
    timeout: Duration::from_millis(100),
    show_progress: false,
    ..ScoringConfig::default()
  };
  let scored = score_one(&dead_proxy(), &config);
  assert_eq!(scored.score.value, 0.0);
  assert_eq!(scored.score.successes, 0);
  assert_eq!(scored.score.attempts, 1);
}

#[test]
fn empty_input_produces_no_results() {
  let scored = score_all(Vec::new(), &ScoringConfig::default());
  assert!(scored.is_empty());
}
