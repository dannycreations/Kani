#![cfg(windows)]

use std::{env, thread};

use anyhow::{anyhow, Result};
use windows::Win32::System::Registry::{
  RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
  HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, REG_VALUE_TYPE,
};
use windows_core::PCWSTR;

const REG_RUN_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const REG_VALUE_NAME: &str = "VolLevelLock";

use crate::utils::to_wide;

pub fn register_autorun() -> Result<()> {
  let executable_path = env::current_exe()?;
  let command_line =
    format!("\"{}\" --hidden", executable_path.to_string_lossy());
  let subkey_utf16 = to_wide(REG_RUN_PATH);
  let value_name_utf16 = to_wide(REG_VALUE_NAME);
  let command_utf16 = to_wide(&command_line);

  unsafe {
    let mut key_handle = windows::Win32::System::Registry::HKEY::default();
    let status = RegCreateKeyExW(
      HKEY_CURRENT_USER,
      PCWSTR(subkey_utf16.as_ptr()),
      Some(0),
      PCWSTR::null(),
      windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
      KEY_WRITE,
      None,
      &mut key_handle,
      None,
    );

    if status.is_err() {
      return Err(anyhow!("Failed to create/open registry key for autorun"));
    }

    let status_val = RegSetValueExW(
      key_handle,
      PCWSTR(value_name_utf16.as_ptr()),
      Some(0),
      REG_SZ,
      Some(std::slice::from_raw_parts(
        command_utf16.as_ptr() as *const u8,
        command_utf16.len() * 2,
      )),
    );

    let _ = windows::Win32::System::Registry::RegCloseKey(key_handle);

    if status_val.is_err() {
      return Err(anyhow!("Failed to write registry value for autorun"));
    }
  }

  // Also attempt to start the command in the background
  let child_path = executable_path;
  let _ = thread::spawn(move || {
    let _ = std::process::Command::new(child_path)
      .arg("--hidden")
      .spawn();
  });

  Ok(())
}

pub fn deregister_autorun() -> Result<()> {
  let subkey_utf16 = to_wide(REG_RUN_PATH);
  let value_name_utf16 = to_wide(REG_VALUE_NAME);

  unsafe {
    let mut key_handle = windows::Win32::System::Registry::HKEY::default();
    let status = RegOpenKeyExW(
      HKEY_CURRENT_USER,
      PCWSTR(subkey_utf16.as_ptr()),
      None,
      KEY_WRITE,
      &mut key_handle,
    );

    if status.is_err() {
      return Ok(()); // Key doesn't exist, we are good
    }

    let _ = RegDeleteValueW(key_handle, PCWSTR(value_name_utf16.as_ptr()));
    let _ = windows::Win32::System::Registry::RegCloseKey(key_handle);
  }

  Ok(())
}

pub fn is_autorun_registered() -> bool {
  let subkey_utf16 = to_wide(REG_RUN_PATH);
  let value_name_utf16 = to_wide(REG_VALUE_NAME);

  unsafe {
    let mut key_handle = windows::Win32::System::Registry::HKEY::default();
    let status = RegOpenKeyExW(
      HKEY_CURRENT_USER,
      PCWSTR(subkey_utf16.as_ptr()),
      None,
      windows::Win32::System::Registry::KEY_READ,
      &mut key_handle,
    );

    if status.is_err() {
      return false;
    }

    let mut value_type = REG_VALUE_TYPE::default();
    let mut data_len = 0u32;
    let status_val = windows::Win32::System::Registry::RegQueryValueExW(
      key_handle,
      PCWSTR(value_name_utf16.as_ptr()),
      None,
      Some(&mut value_type),
      None,
      Some(&mut data_len),
    );

    let _ = windows::Win32::System::Registry::RegCloseKey(key_handle);
    status_val.is_ok()
  }
}
