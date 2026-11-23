use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use proxy_scorer::{
  fetch,
  model::{Proxy, Scheme},
  report, scorer,
  scorer::ScoringConfig,
  system::{self, Endpoint, ProxyConfig},
};

#[derive(Parser)]
#[command(
  name = "proxy-scorer",
  about = "Score proxies locally in parallel and apply the best one system-wide",
  version
)]
struct Args {
  /// URL of the proxy list to fetch (use --input to read a local file instead).
  #[arg(long, default_value = fetch::DEFAULT_SOURCE)]
  url: String,
  /// Read the proxy list from a local file instead of fetching --url.
  #[arg(long, value_name = "FILE")]
  input: Option<PathBuf>,
  /// Maximum number of proxies to test; 0 means all.
  #[arg(long, default_value_t = 200, value_name = "N")]
  limit: usize,
  /// Number of parallel scoring workers; defaults to the CPU count.
  #[arg(long, value_name = "N")]
  workers: Option<usize>,
  /// Attempts per proxy before averaging its latency and success rate.
  #[arg(long, default_value_t = 2, value_name = "N")]
  tries: u32,
  /// Milliseconds allowed for the TCP/TLS connection to a proxy.
  #[arg(long, default_value_t = 3_000, value_name = "MS")]
  connect_timeout: u64,
  /// Milliseconds allowed for each full request through a proxy.
  #[arg(long, default_value_t = 8_000, value_name = "MS")]
  timeout: u64,
  /// Milliseconds allowed to download the proxy list itself.
  #[arg(long, default_value_t = 15_000, value_name = "MS")]
  fetch_timeout: u64,
  /// URL requested through each proxy; 2xx counts as a success.
  #[arg(long, default_value = "https://example.com")]
  test_url: String,
  /// Number of ranked results to display.
  #[arg(long, default_value_t = 20, value_name = "N")]
  top: usize,
  /// Hide results scoring below this value (0-100).
  #[arg(long, default_value_t = 0.0, value_name = "SCORE")]
  min_score: f64,
  /// Keep only these schemes; repeatable (http, https, socks4, socks5).
  #[arg(long, value_name = "SCHEME")]
  scheme: Vec<Scheme>,
  /// Result format: table for humans, json for piping into other tools.
  #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
  format: OutputFormat,
  /// Write results to this file instead of stdout (table mode still prints).
  #[arg(long, value_name = "FILE")]
  output: Option<PathBuf>,
  /// Apply the highest-scoring proxy at the OS level after scoring.
  #[arg(long)]
  apply: bool,
  /// Disable the system proxy and exit without scoring.
  #[arg(long)]
  clear: bool,
  /// Print the current system proxy settings and exit without scoring.
  #[arg(long)]
  show_system: bool,
  /// Set the system HTTP proxy to HOST:PORT and exit without scoring.
  #[arg(long, value_name = "HOST:PORT")]
  set_http: Option<Endpoint>,
  /// Set the system HTTPS proxy to HOST:PORT and exit without scoring.
  #[arg(long, value_name = "HOST:PORT")]
  set_https: Option<Endpoint>,
  /// Set the system SOCKS proxy to HOST:PORT and exit without scoring.
  #[arg(long, value_name = "HOST:PORT")]
  set_socks: Option<Endpoint>,
  /// Turn off the progress bar while scoring.
  #[arg(long)]
  no_progress: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
  /// Human-readable ranked table (default).
  Table,
  /// Machine-readable JSON object with a results array.
  Json,
}

fn to_system_config(proxy: &Proxy) -> Result<ProxyConfig> {
  let endpoint = Endpoint::new(proxy.host.clone(), proxy.port)?;
  Ok(if proxy.scheme.is_socks() {
    ProxyConfig {
      http: None,
      https: None,
      socks: Some(endpoint),
    }
  } else {
    ProxyConfig {
      http: Some(endpoint.clone()),
      https: Some(endpoint),
      socks: None,
    }
  })
}

fn main() -> Result<()> {
  let args = Args::parse();

  if args.clear {
    system::clear()?;
    println!("System proxy disabled.");
    return Ok(());
  }

  if args.show_system {
    let config = system::show()?;
    if config.is_empty() {
      println!("No system proxy configured.");
    } else {
      println!("{}", config.to_proxy_string());
    }
    return Ok(());
  }

  let direct = ProxyConfig {
    http: args.set_http,
    https: args.set_https,
    socks: args.set_socks,
  };
  if !direct.is_empty() {
    system::apply(&direct)?;
    println!("Applied system proxy: {}", direct.to_proxy_string());
    return Ok(());
  }

  let mut proxies = match &args.input {
    Some(path) => {
      let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
      fetch::parse_list(&content)
    }
    None => fetch::fetch(&args.url, Duration::from_millis(args.fetch_timeout))?,
  };

  if !args.scheme.is_empty() {
    proxies.retain(|proxy| args.scheme.contains(&proxy.scheme));
  }
  if args.limit > 0 {
    proxies.truncate(args.limit);
  }
  if proxies.is_empty() {
    bail!("no proxies to score");
  }

  let config = ScoringConfig {
    workers: args.workers,
    tries: args.tries,
    connect_timeout: Duration::from_millis(args.connect_timeout),
    timeout: Duration::from_millis(args.timeout),
    test_url: args.test_url,
    show_progress: !args.no_progress,
  };

  let scored = scorer::score_all(proxies, &config);
  let visible_len = scored
    .iter()
    .take_while(|item| item.score.value >= args.min_score)
    .count()
    .min(args.top);
  let visible = &scored[..visible_len];

  match args.format {
    OutputFormat::Table => {
      report::print_table(visible);
      if let Some(path) = &args.output {
        report::write_file(path, &report::to_json(visible)?)?;
        println!("Wrote {}", path.display());
      }
    }
    OutputFormat::Json => {
      let json = report::to_json(visible)?;
      if let Some(path) = &args.output {
        report::write_file(path, &json)?;
      } else {
        println!("{json}");
      }
    }
  }

  if args.apply {
    let best = scored
      .first()
      .ok_or_else(|| anyhow!("no results to apply"))?;
    let system_config = to_system_config(&best.proxy)?;
    system::apply(&system_config)?;
    println!("Applied best proxy {} at system level.", best.proxy);
  }

  Ok(())
}
