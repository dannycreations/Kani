use std::{os::windows::ffi::OsStrExt, path::Path};

use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::ERROR_SUCCESS,
    System::Registry::{
      RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
      RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
    },
  },
};

const REGISTRY_RUN_KEY: PCWSTR =
  w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run");
const REGISTRY_RUN_VALUE: PCWSTR = w!("BorderlessFullscreen");

pub fn is_autostart_enabled() -> bool {
  unsafe {
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(
      HKEY_CURRENT_USER,
      REGISTRY_RUN_KEY,
      Some(0),
      KEY_READ,
      &mut hkey,
    ) != ERROR_SUCCESS
    {
      return false;
    }

    let exists =
      RegQueryValueExW(hkey, REGISTRY_RUN_VALUE, None, None, None, None)
        .is_ok();
    let _ = RegCloseKey(hkey);
    exists
  }
}

pub fn set_autostart_enabled(enable: bool, exe_path: &Path) {
  unsafe {
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(
      HKEY_CURRENT_USER,
      REGISTRY_RUN_KEY,
      Some(0),
      KEY_SET_VALUE,
      &mut hkey,
    ) != ERROR_SUCCESS
    {
      return;
    }

    if enable {
      // Run entries are written as a quoted path, because an install
      // directory may contain spaces.
      let mut command: Vec<u16> =
        Vec::with_capacity(exe_path.as_os_str().len() + 3);
      command.push(b'"' as u16);
      command.extend(exe_path.as_os_str().encode_wide());
      command.push(b'"' as u16);
      command.push(0);

      let byte_len = std::mem::size_of_val(&command[..]);
      // SAFETY: `REG_SZ` is documented to take a NUL-terminated UTF-16
      // string, so the same bytes are read back as a `&[u8]`. `command` is
      // built above, stays alive for the call, and is NUL-terminated.
      let bytes =
        std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), byte_len);
      let _ =
        RegSetValueExW(hkey, REGISTRY_RUN_VALUE, Some(0), REG_SZ, Some(bytes));
    } else {
      let _ = RegDeleteValueW(hkey, REGISTRY_RUN_VALUE);
    }
    let _ = RegCloseKey(hkey);
  }
}
