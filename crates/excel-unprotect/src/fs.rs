use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn normalize_path(user_input: &str) -> Result<PathBuf> {
  let cleaned = user_input.trim().trim_matches(|c| c == '"' || c == '\'');
  let path = PathBuf::from(cleaned);
  let canonical = path.canonicalize().unwrap_or(path);

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
  let mut new_name =
    String::with_capacity(stem.len() + suffix.len() + extension.len() + 1);
  new_name.push_str(&stem);
  new_name.push_str(suffix);
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

    let capacity = stem.len() + 1 + 10 + 1 + extension.len();
    let mut new_name = String::with_capacity(capacity);
    new_name.push_str(&stem);
    new_name.push('_');
    new_name.push_str(&counter.to_string());
    if !extension.is_empty() {
      new_name.push('.');
      new_name.push_str(&extension);
    }
    final_path = target.with_file_name(new_name);
  }
  final_path
}
