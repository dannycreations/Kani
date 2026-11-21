#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{apply, clear, show};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{apply, clear, show};

#[cfg(all(not(windows), not(target_os = "macos")))]
mod linux;
#[cfg(all(not(windows), not(target_os = "macos")))]
pub use linux::{apply, clear, show};
