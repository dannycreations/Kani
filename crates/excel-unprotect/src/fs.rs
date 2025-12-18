use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};

pub fn normalize_path(user_input: &str) -> Result<PathBuf> {
  let cleaned = user_input.trim().trim_matches(|c| c == '"' || c == '\'');

  if cleaned.is_empty() {
    bail!("Path is empty");
  }

  if cleaned.contains('\0') {
    bail!("Path contains null byte");
  }

  let path = PathBuf::from(cleaned);
  let canonical = path
    .canonicalize()
    .map_err(|e| anyhow!("Invalid path: {}", e))?;

  if canonical.parent().is_none() {
    bail!("Cannot operate on root directory");
  }

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
  let stem = path.file_stem().unwrap_or_default();

  let mut new_name = stem.to_os_string();
  new_name.push(suffix);

  if let Some(ext) = path.extension() {
    new_name.push(".");
    new_name.push(ext);
  }

  new_path.set_file_name(new_name);
  new_path
}

pub fn safe_save_path(target: &Path) -> PathBuf {
  if !target.exists() {
    return target.to_path_buf();
  }

  let stem = target.file_stem().unwrap_or_default();
  let extension = target.extension();
  let parent = target.parent().unwrap_or_else(|| Path::new("."));

  for counter in 1..=u32::MAX {
    let mut new_name = stem.to_os_string();
    new_name.push("_");
    new_name.push(counter.to_string());

    if let Some(ext) = extension {
      new_name.push(".");
      new_name.push(ext);
    }

    let candidate = parent.join(new_name);
    if !candidate.exists() {
      return candidate;
    }
  }

  target.to_path_buf()
}
