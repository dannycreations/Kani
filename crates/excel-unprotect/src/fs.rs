use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn normalize_path(user_input: &str) -> Result<PathBuf> {
  let cleaned = user_input.trim().trim_matches(|c| c == '"' || c == '\'');
  let path = PathBuf::from(cleaned);
  let canonical = path.canonicalize().unwrap_or(path);

  #[cfg(windows)]
  {
    let path_str = canonical.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
      return Ok(PathBuf::from(stripped));
    }
  }

  Ok(canonical)
}

pub fn add_suffix(path: &Path, suffix: &str) -> PathBuf {
  let mut new_path = path.to_path_buf();
  let stem = path.file_stem().unwrap_or_default().to_string_lossy();
  let extension = path.extension();

  let capacity =
    stem.len() + suffix.len() + extension.map_or(0, |e| e.len() + 1);
  let mut new_name = String::with_capacity(capacity);
  new_name.push_str(&stem);
  new_name.push_str(suffix);

  if let Some(ext) = extension {
    new_name.push('.');
    new_name.push_str(&ext.to_string_lossy());
  }

  new_path.set_file_name(new_name);
  new_path
}

pub fn safe_save_path(target: &Path) -> PathBuf {
  if !target.exists() {
    return target.to_path_buf();
  }

  let mut counter = 1;
  let stem = target.file_stem().unwrap_or_default().to_string_lossy();
  let extension = target.extension();
  let parent = target.parent().unwrap_or_else(|| Path::new("."));

  loop {
    let capacity = stem.len() + 5 + extension.map_or(0, |e| e.len() + 1);
    let mut new_name = String::with_capacity(capacity);
    new_name.push_str(&stem);
    new_name.push('_');
    new_name.push_str(&counter.to_string());

    if let Some(ext) = extension {
      new_name.push('.');
      new_name.push_str(&ext.to_string_lossy());
    }

    let candidate = parent.join(new_name);
    if !candidate.exists() {
      return candidate;
    }
    counter += 1;
  }
}
