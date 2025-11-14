use std::io;
use std::io::ErrorKind;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug, PartialEq)]
pub enum Error {
  #[error("I/O error: {0}")]
  Io(ErrorKind, String),
  #[error("Failed to parse patch: {0}")]
  Parse(String),
  #[error("Failed to apply patch: {0}")]
  Apply(String),
  #[error("Unsupported patch type: {0}")]
  Unsupported(String),
}

impl From<io::Error> for Error {
  fn from(err: io::Error) -> Self {
    Self::Io(err.kind(), err.to_string())
  }
}
