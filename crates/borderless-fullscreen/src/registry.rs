use std::{mem::size_of, os::windows::ffi::OsStrExt, path::Path};

use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::ERROR_SUCCESS,
    System::Registry::{
      RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
      RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE,
      REG_SAM_FLAGS, REG_SZ,
    },
  },
};

const REGISTRY_RUN_KEY: PCWSTR =
  w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run");
const REGISTRY_RUN_VALUE: PCWSTR = w!("BorderlessFullscreen");

fn with_run_key<R>(
  access: REG_SAM_FLAGS,
  f: impl FnOnce(HKEY) -> R,
) -> Option<R> {
  // SAFETY: the key path is a module constant, so it outlives the call,
  // and the handle opened here is closed before this function returns.
  unsafe {
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(
      HKEY_CURRENT_USER,
      REGISTRY_RUN_KEY,
      Some(0),
      access,
      &mut hkey,
    ) != ERROR_SUCCESS
    {
      return None;
    }

    let result = f(hkey);
    let _ = RegCloseKey(hkey);
    Some(result)
  }
}

pub fn is_autostart_enabled() -> bool {
  with_run_key(KEY_READ, |hkey| {
    // SAFETY: `hkey` is the open key handle `with_run_key` owns, and the
    // query only reads a value from it.
    unsafe {
      RegQueryValueExW(hkey, REGISTRY_RUN_VALUE, None, None, None, None).is_ok()
    }
  })
  .unwrap_or(false)
}

pub fn set_autostart_enabled(enable: bool, exe_path: &Path) {
  with_run_key(KEY_SET_VALUE, |hkey| {
    // SAFETY: `hkey` is the live key handle `with_run_key` owns for the
    // duration of this call. The value name is a module constant, and the
    // bytes written are the NUL-terminated `command` buffer built below.
    unsafe {
      if enable {
        // Run entries are written as a quoted path, because an install
        // directory may contain spaces.
        let mut command: Vec<u16> =
          Vec::with_capacity(exe_path.as_os_str().len() + 3);
        command.push(b'"' as u16);
        command.extend(exe_path.as_os_str().encode_wide());
        command.push(b'"' as u16);
        command.push(0);

        // SAFETY: `REG_SZ` is documented to take a NUL-terminated UTF-16
        // string, so the same bytes are read back as a `&[u8]`.
        // `command` is built above, stays alive for the call, and is
        // NUL-terminated.
        let bytes = std::slice::from_raw_parts(
          command.as_ptr().cast::<u8>(),
          command.len() * size_of::<u16>(),
        );
        let _ = RegSetValueExW(
          hkey,
          REGISTRY_RUN_VALUE,
          Some(0),
          REG_SZ,
          Some(bytes),
        );
      } else {
        let _ = RegDeleteValueW(hkey, REGISTRY_RUN_VALUE);
      }
    }
  });
}
