#![cfg(windows)]

use std::env;

use anyhow::{anyhow, Result};
use windows::{
  core::PCWSTR,
  Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::{CreateMutexW, ReleaseMutex},
  },
};

use crate::utils::to_wide;

const MUTEX_PREFIX: &str = "Local\\VolumeLevelLock-";

pub struct InstanceGuard(pub HANDLE);

impl Drop for InstanceGuard {
  fn drop(&mut self) {
    unsafe {
      let _ = ReleaseMutex(self.0);
      let _ = CloseHandle(self.0);
    }
  }
}

pub fn acquire_single_instance_guard() -> Result<Option<InstanceGuard>> {
  let username =
    env::var("USERNAME").unwrap_or_else(|_| "UnknownUser".to_string());
  let mutex_name = format!("{}{}", MUTEX_PREFIX, username);
  let mutex_name_utf16 = to_wide(&mutex_name);

  unsafe {
    match CreateMutexW(None, false, PCWSTR(mutex_name_utf16.as_ptr())) {
      Ok(handle) => {
        if GetLastError() == ERROR_ALREADY_EXISTS {
          let _ = CloseHandle(handle);
          Ok(None)
        } else {
          Ok(Some(InstanceGuard(handle)))
        }
      }
      Err(err) => {
        Err(anyhow!("Failed to create single-instance mutex: {:?}", err))
      }
    }
  }
}
