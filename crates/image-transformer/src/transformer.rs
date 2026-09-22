use std::{
  fs,
  io::Error as StdIoError,
  path::{Path, PathBuf},
  str::FromStr,
  time::Duration,
};

use image::{imageops::FilterType, DynamicImage, ImageError};
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use oxipng::{BitDepth, ColorType, Options, RawImage, StripChunks};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TransformerError {
  #[error("Unsupported resolution: {0}")]
  UnsupportedResolution(String),
  #[error("Failed to open image {0}: {1}")]
  OpenError(PathBuf, #[source] ImageError),
  #[error("Failed to optimize PNG: {0}")]
  OptimizationError(String),
  #[error("IO error: {0}")]
  IoError(#[from] StdIoError),
  #[error("Image error: {0}")]
  ImageError(#[from] ImageError),
  #[error("Invalid input: {0}")]
  InvalidInput(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
  P4K,
  P2K,
  P1K,
  P720,
  P480,
  P360,
}

impl FromStr for Resolution {
  type Err = TransformerError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "4k" => Ok(Self::P4K),
      "2k" => Ok(Self::P2K),
      "1k" => Ok(Self::P1K),
      "720" => Ok(Self::P720),
      "480" => Ok(Self::P480),
      "360" => Ok(Self::P360),
      _ => Err(TransformerError::UnsupportedResolution(s.to_string())),
    }
  }
}

impl Resolution {
  pub const fn to_height(self) -> u32 {
    match self {
      Self::P4K => 2160,
      Self::P2K => 1440,
      Self::P1K => 1080,
      Self::P720 => 720,
      Self::P480 => 480,
      Self::P360 => 360,
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

pub struct ProcessResult {
  pub original_size: u64,
  pub optimized_size: u64,
}

impl ProcessResult {
  pub fn compression_ratio(&self) -> f64 {
    if self.original_size == 0 {
      0.0
    } else {
      1.0 - (self.optimized_size as f64 / self.original_size as f64)
    }
  }
}

#[derive(Debug, Default)]
pub struct Summary {
  pub processed: usize,
  pub failed: usize,
  pub original_total: u64,
  pub optimized_total: u64,
}

impl Summary {
  pub fn from_results(
    results: &[Result<ProcessResult, TransformerError>],
  ) -> Self {
    let mut summary = Self {
      processed: results.len(),
      ..Self::default()
    };

    for result in results {
      match result {
        Ok(r) => {
          summary.original_total += r.original_size;
          summary.optimized_total += r.optimized_size;
        }
        Err(_) => summary.failed += 1,
      }
    }

    summary
  }

  pub fn saved_ratio(&self) -> f64 {
    if self.original_total == 0 {
      0.0
    } else {
      1.0 - (self.optimized_total as f64 / self.original_total as f64)
    }
  }
}

fn transform_image(
  img: DynamicImage,
  config: &TransformerConfig,
) -> DynamicImage {
  let (orig_w, orig_h) = (img.width(), img.height());

  let (target_w, target_h) = if let Some(res) = config.scale {
    let target_h = res.to_height();
    if target_h == orig_h {
      return img;
    }
    let target_w =
      (u64::from(orig_w) * u64::from(target_h) / u64::from(orig_h)) as u32;
    (target_w, target_h)
  } else if config.width.is_some() || config.height.is_some() {
    (
      config.width.unwrap_or(orig_w),
      config.height.unwrap_or(orig_h),
    )
  } else {
    return img;
  };

  if target_w == orig_w && target_h == orig_h {
    return img;
  }

  img.resize(target_w, target_h, FilterType::Lanczos3)
}

fn compute_output_path(
  path: &Path,
  config: &TransformerConfig,
) -> Result<PathBuf, TransformerError> {
  let base_name =
    path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
      TransformerError::InvalidInput("Invalid file name".to_string())
    })?;

  let target_dir = config
    .output_dir
    .as_deref()
    .unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")));

  Ok(target_dir.join(format!("{base_name}.png")))
}

fn process_image(
  path: &Path,
  config: &TransformerConfig,
) -> Result<ProcessResult, TransformerError> {
  let output_path = compute_output_path(path, config)?;

  let input_meta = fs::metadata(path)?;
  let original_size = input_meta.len();

  if config.output_dir.is_some() {
    if let Ok(output_meta) = fs::metadata(&output_path) {
      if let (Ok(in_mtime), Ok(out_mtime)) =
        (input_meta.modified(), output_meta.modified())
      {
        if out_mtime >= in_mtime {
          return Ok(ProcessResult {
            original_size,
            optimized_size: output_meta.len(),
          });
        }
      }
    }
  }

  let img = image::open(path)
    .map_err(|e| TransformerError::OpenError(path.to_path_buf(), e))?;
  let transformed = transform_image(img, config).into_rgba8();
  let (width, height) = transformed.dimensions();

  let raw_image = RawImage::new(
    width,
    height,
    ColorType::RGBA,
    BitDepth::Eight,
    transformed.into_raw(),
  )
  .map_err(|e| TransformerError::OptimizationError(e.to_string()))?;

  let mut options = Options::max_compression();
  options.strip = StripChunks::All;

  let optimized_data = raw_image
    .create_optimized_png(&options)
    .map_err(|e| TransformerError::OptimizationError(e.to_string()))?;

  fs::write(&output_path, &optimized_data)?;

  Ok(ProcessResult {
    original_size,
    optimized_size: optimized_data.len() as u64,
  })
}

fn is_supported_extension(path: &Path) -> bool {
  path
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| {
      ext.eq_ignore_ascii_case("jpg")
        || ext.eq_ignore_ascii_case("jpeg")
        || ext.eq_ignore_ascii_case("png")
    })
}

fn collect_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
  inputs
    .par_iter()
    .flat_map(|input| {
      if input.is_file() {
        return vec![input.clone()];
      }

      WalkDir::new(input)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| is_supported_extension(path))
        .collect::<Vec<_>>()
    })
    .collect()
}

pub fn run(
  config: TransformerConfig,
) -> Vec<Result<ProcessResult, TransformerError>> {
  let files = collect_files(&config.inputs);

  if files.is_empty() {
    return Vec::new();
  }

  if let Some(ref dir) = config.output_dir {
    if let Err(e) = fs::create_dir_all(dir) {
      return vec![Err(TransformerError::IoError(e))];
    }
  }

  let pb = ProgressBar::new(files.len() as u64);
  pb.set_style(
    ProgressStyle::with_template("{msg} | {elapsed_precise}")
      .unwrap_or_else(|_| ProgressStyle::default_bar()),
  );
  pb.enable_steady_tick(Duration::from_millis(1000));

  files
    .par_iter()
    .map(|path| {
      let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

      pb.set_message(file_name.clone());

      let res = process_image(path, &config);
      pb.inc(1);

      match res {
        Ok(ref r) => {
          pb.println(format!(
            "{} | {} | {:.2}% saved",
            file_name,
            HumanDuration(pb.elapsed()),
            r.compression_ratio() * 100.0
          ));
        }
        Err(ref e) => {
          pb.println(format!(
            "{} | {} | failed: {}",
            file_name,
            HumanDuration(pb.elapsed()),
            e
          ));
        }
      }
      res
    })
    .collect()
}
