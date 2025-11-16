use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::fs::Permissions;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub trait FileSystem {
  fn read_to_string(&self, path: &Path) -> io::Result<String>;
  fn write(&mut self, path: &Path, contents: &str) -> io::Result<()>;
  fn remove_file(&mut self, path: &Path) -> io::Result<()>;
  fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;
  #[cfg(unix)]
  fn set_permissions(
    &mut self,
    path: &Path,
    perm: Permissions,
  ) -> io::Result<()>;
  #[cfg(unix)]
  fn get_permissions(&self, path: &Path) -> io::Result<Permissions>;
}

#[derive(Debug, Default)]
pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
  fn read_to_string(&self, path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
  }

  fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
    fs::write(path, contents)
  }

  fn remove_file(&mut self, path: &Path) -> io::Result<()> {
    fs::remove_file(path)
  }

  fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
  }

  #[cfg(unix)]
  fn set_permissions(
    &mut self,
    path: &Path,
    perm: Permissions,
  ) -> io::Result<()> {
    fs::set_permissions(path, perm)
  }

  #[cfg(unix)]
  fn get_permissions(&self, path: &Path) -> io::Result<Permissions> {
    fs::metadata(path).map(|metadata| metadata.permissions())
  }
}

#[derive(Debug, Clone, Default)]
pub struct MockFileSystem {
  pub files: HashMap<PathBuf, String>,
  pub created_dirs: Vec<PathBuf>,
  #[cfg(unix)]
  pub file_modes: HashMap<PathBuf, Permissions>,
}

#[allow(dead_code)]
impl MockFileSystem {
  pub fn new(files: HashMap<PathBuf, String>) -> Self {
    Self {
      files,
      ..Default::default()
    }
  }

  pub fn new_with_dirs(
    files: HashMap<PathBuf, String>,
    created_dirs: Vec<PathBuf>,
    #[cfg(unix)] file_modes: HashMap<PathBuf, Permissions>,
  ) -> Self {
    Self {
      files,
      created_dirs,
      #[cfg(unix)]
      file_modes,
    }
  }
}

impl FileSystem for MockFileSystem {
  fn read_to_string(&self, path: &Path) -> io::Result<String> {
    self
      .files
      .get(path)
      .cloned()
      .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))
  }

  fn write(&mut self, path: &Path, contents: &str) -> io::Result<()> {
    self.files.insert(path.to_path_buf(), contents.to_string());
    Ok(())
  }

  fn remove_file(&mut self, path: &Path) -> io::Result<()> {
    if self.files.remove(path).is_some() {
      Ok(())
    } else {
      Err(io::Error::new(io::ErrorKind::NotFound, "file not found"))
    }
  }

  fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
    self.created_dirs.push(path.to_path_buf());
    Ok(())
  }

  #[cfg(unix)]
  fn set_permissions(
    &mut self,
    path: &Path,
    perm: std::fs::Permissions,
  ) -> io::Result<()> {
    self.file_modes.insert(path.to_path_buf(), perm);
    Ok(())
  }

  #[cfg(unix)]
  fn get_permissions(&self, path: &Path) -> io::Result<Permissions> {
    self.file_modes.get(path).cloned().ok_or_else(|| {
      io::Error::new(io::ErrorKind::NotFound, "permissions not found")
    })
  }
}
