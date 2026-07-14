use std::{
  fs::{create_dir_all, metadata, write},
  io::{Cursor, Error as StdIoError},
  marker::PhantomData,
  path::{Path, PathBuf},
  str::FromStr,
  time::Duration,
};

use image::{
  imageops::FilterType, load_from_memory_with_format, open, DynamicImage,
  ImageError, ImageFormat,
};
use indicatif::{ProgressBar, ProgressStyle};
use oxipng::{optimize_from_memory, Options, StripChunks};
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
  #[must_use]
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

pub trait ImageState {}
pub struct Raw;
pub struct Transformed;
pub struct Optimized;
impl ImageState for Raw {}
impl ImageState for Transformed {}
impl ImageState for Optimized {}

pub struct LosslessImage<S: ImageState> {
  inner: DynamicImage,
  _state: PhantomData<S>,
}

impl LosslessImage<Raw> {
  pub fn load(path: &Path) -> Result<Self, TransformerError> {
    let img = open(path)
      .map_err(|e| TransformerError::OpenError(path.to_path_buf(), e))?;
    Ok(Self {
      inner: img,
      _state: PhantomData,
    })
  }

  #[must_use]
  pub fn transform(
    self,
    config: &TransformerConfig,
  ) -> LosslessImage<Transformed> {
    let img = if let Some(res) = config.scale {
      let h = res.to_height();
      if h != self.inner.height() {
        let w = (self.inner.width() as u64 * h as u64
          / self.inner.height() as u64) as u32;
        self.inner.resize(w, h, FilterType::Lanczos3)
      } else {
        self.inner
      }
    } else if config.width.is_some() || config.height.is_some() {
      let w = config.width.unwrap_or_else(|| self.inner.width());
      let h = config.height.unwrap_or_else(|| self.inner.height());
      self.inner.resize(w, h, FilterType::Lanczos3)
    } else {
      self.inner
    };

    LosslessImage {
      inner: img,
      _state: PhantomData,
    }
  }
}

impl LosslessImage<Transformed> {
  pub fn optimize(self) -> Result<LosslessImage<Optimized>, TransformerError> {
    let mut buffer = Vec::new();
    self
      .inner
      .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)?;

    let mut options = Options::max_compression();
    options.strip = StripChunks::All;

    let optimized_data = optimize_from_memory(&buffer, &options)
      .map_err(|e| TransformerError::OptimizationError(e.to_string()))?;

    let optimized_img =
      load_from_memory_with_format(&optimized_data, ImageFormat::Png)?;

    Ok(LosslessImage {
      inner: optimized_img,
      _state: PhantomData,
    })
  }
}

impl LosslessImage<Optimized> {
  pub fn save(self, path: &Path) -> Result<u64, TransformerError> {
    let mut buffer = Vec::new();
    self
      .inner
      .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)?;

    write(path, &buffer)?;
    Ok(buffer.len() as u64)
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
  pub path: PathBuf,
  pub original_size: u64,
  pub optimized_size: u64,
}

impl ProcessResult {
  #[must_use]
  pub fn compression_ratio(&self) -> f64 {
    if self.original_size == 0 {
      0.0
    } else {
      1.0 - (self.optimized_size as f64 / self.original_size as f64)
    }
  }
}

fn process_image(
  path: &Path,
  config: &TransformerConfig,
) -> Result<ProcessResult, TransformerError> {
  let original_size = metadata(path)?.len();

  let raw = LosslessImage::load(path)?;
  let transformed = raw.transform(config);
  let optimized = transformed.optimize()?;

  let mut output_path = if let Some(ref dir) = config.output_dir {
    if !dir.exists() {
      create_dir_all(dir)?;
    }
    dir.join(path.file_name().ok_or_else(|| {
      TransformerError::InvalidInput("Invalid file name".to_string())
    })?)
  } else {
    path.to_path_buf()
  };
  output_path.set_extension("png");

  let optimized_size = optimized.save(&output_path)?;

  Ok(ProcessResult {
    path: path.to_path_buf(),
    original_size,
    optimized_size,
  })
}

fn format_precise(d: Duration) -> String {
  let secs = d.as_secs();
  format!(
    "{:02}:{:02}:{:02}",
    secs / 3600,
    (secs % 3600) / 60,
    secs % 60
  )
}

#[must_use]
pub fn run(
  config: TransformerConfig,
) -> Vec<Result<ProcessResult, TransformerError>> {
  let files: Vec<PathBuf> = config
    .inputs
    .iter()
    .flat_map(|input| {
      if input.is_file() {
        vec![input.clone()]
      } else {
        WalkDir::new(input)
          .into_iter()
          .filter_map(|e| e.ok())
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
    return Vec::new();
  }

  let pb = ProgressBar::new(files.len() as u64);
  pb.set_style(
    ProgressStyle::with_template("[{elapsed_precise}] {pos}/{len} {msg}")
      .unwrap_or_else(|_| ProgressStyle::default_bar()),
  );
  pb.enable_steady_tick(Duration::from_millis(100));

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
            "[{}] {}/{} {} ({:.2}% saved)",
            format_precise(pb.elapsed()),
            pb.position(),
            files.len(),
            file_name,
            r.compression_ratio() * 100.0
          ));
        }
        Err(ref e) => {
          pb.println(format!(
            "[{}] {}/{} {} failed: {}",
            format_precise(pb.elapsed()),
            pb.position(),
            files.len(),
            file_name,
            e
          ));
        }
      }
      res
    })
    .collect()
}
