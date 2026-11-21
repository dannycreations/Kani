use std::{iter, mem::size_of, slice};

use anyhow::{bail, Context, Result};
use windows::{
  core::{w, PCWSTR},
  Win32::{
    Networking::WinInet::{
      InternetSetOptionW, INTERNET_OPTION_REFRESH,
      INTERNET_OPTION_SETTINGS_CHANGED,
    },
    System::Registry::{
      RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
      HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_SAM_FLAGS, REG_SZ,
    },
  },
};

use crate::system::config::ProxyConfig;

const SETTINGS_PATH: PCWSTR =
  w!("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
const VALUE_PROXY_ENABLE: PCWSTR = w!("ProxyEnable");
const VALUE_PROXY_SERVER: PCWSTR = w!("ProxyServer");
const VALUE_PROXY_OVERRIDE: PCWSTR = w!("ProxyOverride");

fn with_settings_key<R>(
  access: REG_SAM_FLAGS,
  f: impl FnOnce(HKEY) -> Result<R>,
) -> Result<R> {
  // SAFETY: the key path is a module constant and the opened handle is
  // closed before this function returns on every path.
  unsafe {
    let mut key = HKEY::default();
    let status = RegOpenKeyExW(
      HKEY_CURRENT_USER,
      SETTINGS_PATH,
      Some(0),
      access,
      &mut key,
    );
    if status.is_err() {
      bail!("failed to open Internet Settings registry key");
    }

    let result = f(key);
    let _ = RegCloseKey(key);
    result
  }
}

fn set_dword(key: HKEY, name: PCWSTR, value: u32) -> Result<()> {
  // SAFETY: `value` is a four-byte value that stays alive for the call,
  // which is exactly what REG_DWORD requires.
  unsafe {
    let bytes = slice::from_raw_parts(
      &value as *const u32 as *const u8,
      size_of::<u32>(),
    );
    RegSetValueExW(key, name, Some(0), REG_DWORD, Some(bytes))
      .ok()
      .context("failed to write a proxy registry value")
  }
}

fn set_string(key: HKEY, name: PCWSTR, value: &str) -> Result<()> {
  let wide: Vec<u16> = value.encode_utf16().chain(iter::once(0)).collect();
  // SAFETY: `wide` is NUL-terminated and outlives the call, and the byte
  // slice is a faithful reinterpretation of the UTF-16 buffer, which is
  // what REG_SZ expects.
  unsafe {
    let bytes = slice::from_raw_parts(
      wide.as_ptr().cast::<u8>(),
      wide.len() * size_of::<u16>(),
    );
    RegSetValueExW(key, name, Some(0), REG_SZ, Some(bytes))
      .ok()
      .context("failed to write a proxy registry value")
  }
}

fn read_string(key: HKEY, name: PCWSTR) -> Option<String> {
  let mut buffer = [0u16; 1024];
  let mut length = (buffer.len() * size_of::<u16>()) as u32;
  // SAFETY: the buffer is writable for `length` bytes and stays alive for
  // the call; RegQueryValueExW reports the bytes actually written in
  // `length`.
  unsafe {
    if RegQueryValueExW(
      key,
      name,
      None,
      None,
      Some(buffer.as_mut_ptr().cast::<u8>()),
      Some(&mut length),
    )
    .is_err()
    {
      return None;
    }
  }

  let count = length as usize / size_of::<u16>();
  if count == 0 || count > buffer.len() {
    return None;
  }
  Some(
    String::from_utf16_lossy(&buffer[..count])
      .trim_end_matches('\0')
      .to_owned(),
  )
}

fn read_dword(key: HKEY, name: PCWSTR) -> Option<u32> {
  let mut value = 0u32;
  let mut length = size_of::<u32>() as u32;
  // SAFETY: `value` is writable for `length` bytes and outlives the call.
  unsafe {
    if RegQueryValueExW(
      key,
      name,
      None,
      None,
      Some((&mut value as *mut u32).cast::<u8>()),
      Some(&mut length),
    )
    .is_err()
    {
      return None;
    }
  }
  Some(value)
}

fn refresh() {
  // SAFETY: INTERNET_OPTION_SETTINGS_CHANGED and INTERNET_OPTION_REFRESH
  // are documented to operate globally and accept a null handle.
  unsafe {
    let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
    let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
  }
}

pub fn apply(config: &ProxyConfig) -> Result<()> {
  if config.is_empty() {
    bail!("no proxy endpoints provided");
  }

  let proxy_string = config.to_proxy_string();
  with_settings_key(KEY_READ | KEY_WRITE, |key| {
    set_string(key, VALUE_PROXY_SERVER, &proxy_string)?;
    set_string(key, VALUE_PROXY_OVERRIDE, "<local>")?;
    set_dword(key, VALUE_PROXY_ENABLE, 1)
  })?;

  refresh();
  Ok(())
}

pub fn clear() -> Result<()> {
  with_settings_key(KEY_READ | KEY_WRITE, |key| {
    set_dword(key, VALUE_PROXY_ENABLE, 0)
  })?;

  refresh();
  Ok(())
}

pub fn show() -> Result<ProxyConfig> {
  with_settings_key(KEY_READ, |key| {
    if read_dword(key, VALUE_PROXY_ENABLE).unwrap_or(0) == 0 {
      return Ok(ProxyConfig::default());
    }
    let value = read_string(key, VALUE_PROXY_SERVER).unwrap_or_default();
    Ok(ProxyConfig::from_proxy_string(&value))
  })
}
