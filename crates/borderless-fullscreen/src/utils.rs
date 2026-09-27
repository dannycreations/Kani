use std::{os::windows::ffi::OsStrExt, path::Path, rc::Rc};

use windows::{
  core::{w, PCWSTR, PWSTR},
  Win32::{
    Foundation::{CloseHandle, HWND},
    System::Threading::{
      OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
      PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
      Shell::ShellExecuteW,
      WindowsAndMessaging::{
        GetWindowTextLengthW, GetWindowTextW, IsWindow, SW_SHOWNORMAL,
      },
    },
  },
};

#[inline(always)]
pub fn is_valid_window(hwnd: HWND) -> bool {
  !hwnd.0.is_null() && unsafe { IsWindow(Some(hwnd)).as_bool() }
}

pub fn get_window_title(hwnd: HWND) -> Box<str> {
  const STACK_BUF_LEN: usize = 256;
  unsafe {
    let mut buffer = [0u16; STACK_BUF_LEN];
    let read = GetWindowTextW(hwnd, &mut buffer);
    if read == 0 {
      return Box::default();
    }
    let read_len = read as usize;

    // A full buffer is indistinguishable from a truncated read, so confirm
    // the real length and re-read into a right-sized buffer if it does not
    // fit. Otherwise the captured units are already the whole title.
    if read_len == STACK_BUF_LEN - 1 {
      let length = GetWindowTextLengthW(hwnd);
      if length as usize >= STACK_BUF_LEN {
        let mut heap_buffer = vec![0u16; length as usize + 1];
        let read2 = GetWindowTextW(hwnd, &mut heap_buffer);
        return String::from_utf16_lossy(&heap_buffer[..read2 as usize])
          .into_boxed_str();
      }
    }

    String::from_utf16_lossy(&buffer[..read_len]).into_boxed_str()
  }
}

pub fn get_process_name(pid: u32) -> Option<Rc<str>> {
  let handle =
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
      .ok()?;
  let mut buffer = [0u16; 512];
  let mut size = buffer.len() as u32;

  let success = unsafe {
    QueryFullProcessImageNameW(
      handle,
      PROCESS_NAME_FORMAT(0),
      PWSTR(buffer.as_mut_ptr()),
      &mut size,
    )
  };
  let _ = unsafe { CloseHandle(handle) };

  if success.is_err() || size == 0 {
    return None;
  }

  let path = &buffer[..size as usize];
  let file_start = path
    .iter()
    .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16)
    .map_or(0, |idx| idx + 1);

  let file = &path[file_start..];
  let stem_len = file
    .iter()
    .rposition(|&c| c == b'.' as u16)
    .unwrap_or(file.len());
  let stem = &file[..stem_len];

  if stem.is_empty() {
    return None;
  }

  Some(Rc::from(String::from_utf16_lossy(stem)))
}

pub fn open_in_default_editor(path: &Path) {
  let wide: Vec<u16> = path
    .as_os_str()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect();
  unsafe {
    ShellExecuteW(
      Some(HWND::default()),
      w!("open"),
      PCWSTR(wide.as_ptr()),
      PCWSTR::null(),
      PCWSTR::null(),
      SW_SHOWNORMAL,
    );
  }
}
