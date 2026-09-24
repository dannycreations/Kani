use std::{
  cell::RefCell, fmt::Write as _, os::windows::ffi::OsStrExt, path::Path,
  rc::Rc,
};

use windows::{
  core::{w, PCWSTR, PWSTR},
  Win32::{
    Foundation::{CloseHandle, HWND},
    Storage::FileSystem::WriteFile,
    System::{
      Console::{
        GetConsoleWindow, GetStdHandle, WriteConsoleW, STD_OUTPUT_HANDLE,
      },
      SystemInformation::GetLocalTime,
      Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
      },
    },
    UI::{
      Shell::ShellExecuteW,
      WindowsAndMessaging::{
        GetWindowTextLengthW, GetWindowTextW, IsWindow, ShowWindow, SW_HIDE,
        SW_SHOW, SW_SHOWNORMAL,
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
    if read_len < STACK_BUF_LEN - 1 {
      let slice = &buffer[..read_len];
      if slice.iter().all(|&c| c <= 127) {
        let mut bytes = Vec::with_capacity(read_len);
        for &c in slice {
          bytes.push(c as u8);
        }
        let s = String::from_utf8_unchecked(bytes);
        return s.into_boxed_str();
      }
      return String::from_utf16_lossy(slice).into_boxed_str();
    }

    let length = GetWindowTextLengthW(hwnd);
    if (length as usize) < STACK_BUF_LEN {
      return String::from_utf16_lossy(&buffer[..read_len]).into_boxed_str();
    }
    let mut heap_buffer = vec![0u16; (length + 1) as usize];
    let read2 = GetWindowTextW(hwnd, &mut heap_buffer);
    String::from_utf16_lossy(&heap_buffer[..read2 as usize]).into_boxed_str()
  }
}

pub fn get_process_name(pid: u32) -> Option<Rc<str>> {
  unsafe {
    let handle =
      OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;

    let success = QueryFullProcessImageNameW(
      handle,
      PROCESS_NAME_FORMAT(0),
      PWSTR(buffer.as_mut_ptr()),
      &mut size,
    );
    let _ = CloseHandle(handle);

    if success.is_err() || size == 0 {
      return None;
    }

    let path_slice = &buffer[..size as usize];
    let file_start = path_slice
      .iter()
      .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16)
      .map_or(0, |idx| idx + 1);

    let file_slice = &path_slice[file_start..];
    let stem_len = file_slice
      .iter()
      .rposition(|&c| c == b'.' as u16)
      .unwrap_or(file_slice.len());

    let stem = &file_slice[..stem_len];
    if stem.iter().all(|&c| c <= 127) {
      let mut bytes = Vec::with_capacity(stem.len());
      for &c in stem {
        bytes.push(c as u8);
      }
      let s = String::from_utf8_unchecked(bytes);
      Some(Rc::from(s.into_boxed_str()))
    } else {
      Some(Rc::from(String::from_utf16_lossy(stem).into_boxed_str()))
    }
  }
}

#[inline]
pub fn set_console_visibility(show: bool) {
  unsafe {
    let hwnd = GetConsoleWindow();
    if !hwnd.0.is_null() {
      let _ = ShowWindow(hwnd, if show { SW_SHOW } else { SW_HIDE });
    }
  }
}

pub fn open_in_default_editor(path: &Path) {
  let os_str = path.as_os_str();
  let wide_len = os_str.len() + 1;
  if wide_len <= 512 {
    let mut buf = [0u16; 512];
    let mut i = 0;
    for c in os_str.encode_wide() {
      buf[i] = c;
      i += 1;
    }
    buf[i] = 0;
    unsafe {
      ShellExecuteW(
        Some(HWND::default()),
        w!("open"),
        PCWSTR(buf.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        SW_SHOWNORMAL,
      );
    }
  } else {
    let wide: Vec<u16> =
      os_str.encode_wide().chain(std::iter::once(0)).collect();
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
}

struct LogBuffers {
  line: String,
  wide: Vec<u16>,
}

thread_local! {
  static LOG_BUFFERS: RefCell<LogBuffers> = RefCell::new(LogBuffers {
    line: String::with_capacity(256),
    wide: Vec::with_capacity(256),
  });
}

pub fn console_log(args: std::fmt::Arguments<'_>) {
  let hwnd = unsafe { GetConsoleWindow() };
  if hwnd.0.is_null() {
    return;
  }

  let tm = unsafe { GetLocalTime() };

  LOG_BUFFERS.with(|cell| {
    let b = &mut *cell.borrow_mut();
    b.line.clear();
    let _ = write!(
      b.line,
      "[{:02}:{:02}:{:02}] {}\r\n",
      tm.wHour, tm.wMinute, tm.wSecond, args
    );

    b.wide.clear();
    b.wide.extend(b.line.encode_utf16());

    unsafe {
      if let Ok(handle) = GetStdHandle(STD_OUTPUT_HANDLE) {
        if !handle.0.is_null()
          && WriteConsoleW(handle, &b.wide, None, None).is_err()
        {
          let _ = WriteFile(handle, Some(b.line.as_bytes()), None, None);
        }
      }
    }
  });
}

#[macro_export]
macro_rules! clog {
  ($($arg:tt)*) => {
    $crate::utils::console_log(format_args!($($arg)*))
  };
}
