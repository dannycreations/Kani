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
      let exe_os = exe_path.as_os_str();
      let mut val_wide = Vec::with_capacity(exe_os.len() + 3);
      val_wide.push(b'"' as u16);
      val_wide.extend(exe_os.encode_wide());
      val_wide.push(b'"' as u16);
      val_wide.push(0);

      let byte_len = val_wide.len() * std::mem::size_of::<u16>();
      let _ = RegSetValueExW(
        hkey,
        REGISTRY_RUN_VALUE,
        Some(0),
        REG_SZ,
        Some(std::slice::from_raw_parts(
          val_wide.as_ptr().cast::<u8>(),
          byte_len,
        )),
      );
    } else {
      let _ = RegDeleteValueW(hkey, REGISTRY_RUN_VALUE);
    }
    let _ = RegCloseKey(hkey);
  }
}
