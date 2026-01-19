use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use image_transformer::{run, Resolution, TransformerConfig};

#[derive(Parser, Debug)]
#[command(
  author,
  version,
  about = "Batch image transformer and optimizer",
  long_about = "A tool to downscale and optimize JPG/PNG images using Lanczos3 resampling and oxipng compression."
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

  let _results = run(config);

  Ok(())
}
