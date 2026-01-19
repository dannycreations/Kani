use std::{
  fs::{create_dir_all, write},
  io::{Cursor, Error, ErrorKind},
  path::{Path, PathBuf},
  str::FromStr,
};

use image::{
  imageops::FilterType, open, DynamicImage, ImageError, ImageFormat,
};
use oxipng::{optimize_from_memory, Options, StripChunks};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use thiserror::Error as ThisError;
use walkdir::WalkDir;

#[derive(ThisError, Debug)]
pub enum TransformerError {
  #[error("Unsupported resolution: {0}")]
  UnsupportedResolution(String),
  #[error("Failed to open image {0}: {1}")]
  OpenError(PathBuf, #[source] ImageError),
  #[error("Failed to optimize PNG: {0}")]
  OptimizationError(String),
  #[error("IO error: {0}")]
  IoError(#[from] Error),
  #[error("Image error: {0}")]
  ImageError(#[from] ImageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
  P4K,
  P2K,
  P1080,
  P720,
  P480,
  P360,
}

impl FromStr for Resolution {
  type Err = TransformerError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "4k" => Ok(Resolution::P4K),
      "2k" => Ok(Resolution::P2K),
      "1080" => Ok(Resolution::P1080),
      "720" => Ok(Resolution::P720),
      "480" => Ok(Resolution::P480),
      "360" => Ok(Resolution::P360),
      _ => Err(TransformerError::UnsupportedResolution(s.to_string())),
    }
  }
}

impl Resolution {
  #[must_use]
  pub const fn to_height(self) -> u32 {
    match self {
      Resolution::P4K => 2160,
      Resolution::P2K => 1440,
      Resolution::P1080 => 1080,
      Resolution::P720 => 720,
      Resolution::P480 => 480,
      Resolution::P360 => 360,
    }
  }
}

#[derive(Debug, Default)]
pub struct TransformerConfig {
  pub inputs: Vec<PathBuf>,
  pub output_dir: Option<PathBuf>,
  pub width: Option<u32>,
  pub height: Option<u32>,
  pub scale: Option<Resolution>,
}

pub fn run(config: TransformerConfig) -> Result<(), TransformerError> {
  let files: Vec<PathBuf> = config
    .inputs
    .iter()
    .flat_map(|input| {
      if input.is_file() {
        vec![input.clone()]
      } else {
        WalkDir::new(input)
          .into_iter()
          .filter_map(Result::ok)
          .filter(|e| e.file_type().is_file())
          .map(|e| e.path().to_path_buf())
          .filter(|path| {
            path
              .extension()
              .and_then(|ext| ext.to_str())
              .map(|ext| {
                let ext = ext.to_lowercase();
                ext == "jpg" || ext == "jpeg" || ext == "png"
              })
              .unwrap_or(false)
          })
          .collect()
      }
    })
    .collect();

  if files.is_empty() {
    return Ok(());
  }

  files.par_iter().for_each(|path| {
    if let Err(e) = process_image(path, &config) {
      eprintln!("Error processing {path:?}: {e}");
    }
  });

  Ok(())
}

fn process_image(
  path: &Path,
  config: &TransformerConfig,
) -> Result<(), TransformerError> {
  let img = open(path)
    .map_err(|e| TransformerError::OpenError(path.to_path_buf(), e))?;

  let processed_img = if let Some(res) = config.scale {
    let h = res.to_height();
    if h != img.height() {
      let w = (img.width() as u64 * h as u64 / img.height() as u64) as u32;
      img.resize(w, h, FilterType::Lanczos3)
    } else {
      img
    }
  } else if config.width.is_some() || config.height.is_some() {
    let w = config.width.unwrap_or(img.width());
    let h = config.height.unwrap_or(img.height());
    img.resize(w, h, FilterType::Lanczos3)
  } else {
    img
  };

  let mut output_path = if let Some(ref dir) = config.output_dir {
    if !dir.exists() {
      create_dir_all(dir)?;
    }
    dir.join(path.file_name().ok_or_else(|| {
      Error::new(ErrorKind::InvalidInput, "Invalid file name")
    })?)
  } else {
    path.to_path_buf()
  };
  output_path.set_extension("png");

  compress_png(processed_img, &output_path)
}

fn compress_png(
  img: DynamicImage,
  output_path: &Path,
) -> Result<(), TransformerError> {
  let mut buffer = Vec::new();
  img.write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)?;

  let mut options = Options::max_compression();
  options.strip = StripChunks::All;

  let optimized = optimize_from_memory(&buffer, &options)
    .map_err(|e| TransformerError::OptimizationError(e.to_string()))?;

  write(output_path, optimized)?;

  Ok(())
}
