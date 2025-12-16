use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn normalize_path(user_input: &str) -> Result<PathBuf> {
  let cleaned = user_input.trim().trim_matches('"').trim_matches('\'');
  let path = PathBuf::from(cleaned);
  let canonical = path.canonicalize().unwrap_or(path);

  // Strip Windows extended path prefix if present
  let path_str = canonical.to_string_lossy();
  if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
    Ok(PathBuf::from(stripped))
  } else {
    Ok(canonical)
  }
}

pub fn add_suffix(path: &Path, suffix: &str) -> PathBuf {
  let stem = path.file_stem().unwrap_or_default().to_string_lossy();
  let extension = path.extension().unwrap_or_default().to_string_lossy();
  let mut new_name = format!("{}{}", stem, suffix);
  if !extension.is_empty() {
    new_name.push('.');
    new_name.push_str(&extension);
  }
  path.with_file_name(new_name)
}

pub fn safe_save_path(target: &Path) -> PathBuf {
  let mut counter = 1;
  let mut final_path = target.to_path_buf();

  while final_path.exists() {
    counter += 1;
    let stem = target.file_stem().unwrap_or_default().to_string_lossy();
    let extension = target.extension().unwrap_or_default().to_string_lossy();
    let mut new_name = format!("{}_{}", stem, counter);
    if !extension.is_empty() {
      new_name.push('.');
      new_name.push_str(&extension);
    }
    final_path = target.with_file_name(new_name);
  }
  final_path
}
