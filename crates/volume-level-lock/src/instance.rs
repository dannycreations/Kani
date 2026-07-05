#![cfg(windows)]

use std::env;

use anyhow::{anyhow, Result};
use windows::Win32::{
  Foundation::{CloseHandle, HANDLE},
  System::Threading::{
    CreateMutexW, OpenMutexW, ReleaseMutex, SYNCHRONIZATION_ACCESS_RIGHTS,
  },
};
use windows_core::PCWSTR;

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
    // Try opening first to see if it already exists
    if let Ok(existing_handle) = OpenMutexW(
      SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000), // SYNCHRONIZE
      false,
      PCWSTR(mutex_name_utf16.as_ptr()),
    ) {
      let _ = CloseHandle(existing_handle);
      // Instance already exists, return None to signal exit
      return Ok(None);
    }

    let mutex_handle =
      CreateMutexW(None, false, PCWSTR(mutex_name_utf16.as_ptr()));
    match mutex_handle {
      Ok(handle) => Ok(Some(InstanceGuard(handle))),
      Err(err) => {
        Err(anyhow!("Failed to create single-instance mutex: {:?}", err))
      }
    }
  }
}
