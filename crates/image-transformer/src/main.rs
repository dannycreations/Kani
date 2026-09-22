use std::path::PathBuf;

mod transformer;

use anyhow::Result;
use clap::Parser;
use transformer::{run, Resolution, Summary, TransformerConfig};

#[derive(Parser, Debug)]
#[command(
  author,
  version,
  about = "Batch image transformer and optimizer",
  long_about = "A tool to downscale and optimize JPG/PNG images"
)]
struct Args {
  /// Input files or directories (supports drag and drop)
  #[arg(required = true, value_name = "INPUT")]
  inputs: Vec<PathBuf>,

  /// Output directory (defaults to overwriting input files)
  #[arg(short, long, value_name = "DIR")]
  output_dir: Option<PathBuf>,

  /// Target width for downscaling
  #[arg(short = 'W', long, value_name = "PIXELS")]
  width: Option<u32>,

  /// Target height for downscaling
  #[arg(short = 'H', long, value_name = "PIXELS")]
  height: Option<u32>,

  /// Pre-built resolution scale
  #[arg(short, long, value_name = "RES")]
  scale: Option<Resolution>,
}

fn main() -> Result<()> {
  let args = Args::parse();

  let config = TransformerConfig {
    inputs: args.inputs,
    output_dir: args.output_dir,
    width: args.width,
    height: args.height,
    scale: args.scale,
  };

  let results = run(config);

  if results.is_empty() {
    println!("No matching images found.");
    return Ok(());
  }

  let summary = Summary::from_results(&results);

  println!(
    "Processed {} file(s), {} failed. {} -> {} bytes ({:.2}% saved)",
    summary.processed,
    summary.failed,
    summary.original_total,
    summary.optimized_total,
    summary.saved_ratio() * 100.0
  );

  if summary.failed > 0 {
    std::process::exit(1);
  }

  Ok(())
}
