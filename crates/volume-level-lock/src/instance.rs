#![cfg(windows)]

use std::env;

use anyhow::Result;
use windows::{
  core::PCWSTR,
  Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::CreateMutexW,
  },
};

use crate::utils::to_wide;

const MUTEX_PREFIX: &str = "Local\\VolumeLevelLock-";

pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
  fn drop(&mut self) {
    // SAFETY: `self.0` is the handle returned by the `CreateMutexW` in
    // `acquire_single_instance_guard`, it is closed exactly once here,
    // and nothing else in the process holds a copy of it.
    unsafe {
      let _ = CloseHandle(self.0);
    }
  }
}

pub fn acquire_single_instance_guard() -> Result<Option<InstanceGuard>> {
  let username =
    env::var("USERNAME").unwrap_or_else(|_| "UnknownUser".to_string());
  let mutex_name = format!("{}{}", MUTEX_PREFIX, username);
  let mutex_name_utf16 = to_wide(&mutex_name);

  // SAFETY: `mutex_name_utf16` is a NUL-terminated UTF-16 buffer built
  // above and stays alive for the call. The handle it returns is either
  // stored in an `InstanceGuard` or closed on the spot.
  let handle =
    unsafe { CreateMutexW(None, false, PCWSTR(mutex_name_utf16.as_ptr()))? };

  // A second instance finding the name already taken means the first one
  // still holds the handle open.
  // SAFETY: `GetLastError` reads thread-local Win32 state and has no
  // preconditions.
  if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
    // SAFETY: `handle` was just created and is not stored anywhere else.
    unsafe {
      let _ = CloseHandle(handle);
    }
    return Ok(None);
  }

  Ok(Some(InstanceGuard(handle)))
}
