#![cfg(windows)]

use std::{
  env, mem::size_of, os::windows::ffi::OsStrExt, process::Command, slice,
};

use anyhow::{bail, Result};
use windows::{
  core::{w, PCWSTR},
  Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
  },
};

const REG_RUN_PATH: PCWSTR =
  w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const REG_VALUE_NAME: PCWSTR = w!("VolumeLevelLock");

fn with_run_key<R>(
  access: REG_SAM_FLAGS,
  f: impl FnOnce(HKEY) -> R,
) -> Option<R> {
  // SAFETY: the key path is a module constant, so it outlives the call,
  // and the handle opened here is closed before this function returns.
  unsafe {
    let mut key_handle = HKEY::default();
    if RegOpenKeyExW(
      HKEY_CURRENT_USER,
      REG_RUN_PATH,
      Some(0),
      access,
      &mut key_handle,
    )
    .is_err()
    {
      return None;
    }

    let result = f(key_handle);
    let _ = RegCloseKey(key_handle);
    Some(result)
  }
}

pub fn register_autorun() -> Result<()> {
  let executable_path = env::current_exe()?;

  // Run entries are stored as a quoted path, because an install
  // directory may contain spaces.
  let mut command: Vec<u16> =
    Vec::with_capacity(executable_path.as_os_str().len() + 3);
  command.push(b'"' as u16);
  command.extend(executable_path.as_os_str().encode_wide());
  command.push(b'"' as u16);
  command.push(0);

  // SAFETY: `key_handle` is only used after `RegCreateKeyExW` reports
  // success, and it is closed on every path out of this block. The key
  // path and value name are module constants.
  unsafe {
    let mut key_handle = HKEY::default();
    if RegCreateKeyExW(
      HKEY_CURRENT_USER,
      REG_RUN_PATH,
      Some(0),
      PCWSTR::null(),
      REG_OPTION_NON_VOLATILE,
      KEY_WRITE,
      None,
      &mut key_handle,
      None,
    )
    .is_err()
    {
      bail!("Failed to create/open registry key for autorun");
    }

    // SAFETY: `REG_SZ` is documented to take a NUL-terminated UTF-16
    // string, so the same bytes are read back as a `&[u8]`. `command` is
    // built above, stays alive for the call, and is NUL-terminated.
    let bytes = slice::from_raw_parts(
      command.as_ptr().cast::<u8>(),
      command.len() * size_of::<u16>(),
    );
    let result =
      RegSetValueExW(key_handle, REG_VALUE_NAME, Some(0), REG_SZ, Some(bytes));
    let _ = RegCloseKey(key_handle);

    if result.is_err() {
      bail!("Failed to write registry value for autorun");
    }
  }

  // Start the app so a fresh install is running right away. When an
  // instance is already up, the single-instance check ends this copy
  // immediately, so the spawn is best effort either way.
  let _ = Command::new(&executable_path).spawn();

  Ok(())
}

pub fn deregister_autorun() -> Result<()> {
  // A missing key means the value is already gone.
  with_run_key(KEY_WRITE, |key_handle| {
    // SAFETY: `key_handle` is the open key handle `with_run_key` owns
    // for the duration of this call, and the value name is a module
    // constant.
    unsafe {
      let _ = RegDeleteValueW(key_handle, REG_VALUE_NAME);
    }
  });

  Ok(())
}

pub fn is_autorun_registered() -> bool {
  with_run_key(KEY_READ, |key_handle| {
    // SAFETY: `key_handle` is the open key handle `with_run_key` owns
    // for the duration of this call, and the query only reads metadata
    // about a value, writing nothing outside the two out-parameters.
    unsafe {
      let mut value_type = REG_VALUE_TYPE::default();
      let mut data_len = 0u32;
      RegQueryValueExW(
        key_handle,
        REG_VALUE_NAME,
        None,
        Some(&mut value_type),
        None,
        Some(&mut data_len),
      )
      .is_ok()
    }
  })
  .unwrap_or(false)
}
