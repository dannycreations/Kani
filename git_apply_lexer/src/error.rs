use std::borrow::Cow;
use std::io;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum Error {
  #[error("Command-line argument error: {0}")]
  Clap(String),
  #[error("I/O error: {0}")]
  Io(io::ErrorKind, String),
  #[error("Failed to parse patch: {0}")]
  Parse(Cow<'static, str>),
  #[error("Failed to apply patch: {0}")]
  Apply(String),
  #[error("Unsupported patch type: {0}")]
  Unsupported(Cow<'static, str>),
}

impl From<io::Error> for Error {
  fn from(err: io::Error) -> Self {
    Self::Io(err.kind(), err.to_string())
  }
}

impl From<clap::Error> for Error {
  fn from(err: clap::Error) -> Self {
    Self::Clap(err.to_string())
  }
}
