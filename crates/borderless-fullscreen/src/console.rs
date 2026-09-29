use std::{cell::RefCell, fmt::Arguments};

use windows::Win32::{
  Foundation::HANDLE,
  Storage::FileSystem::WriteFile,
  System::{
    Console::{
      AttachConsole, GetStdHandle, WriteConsoleW, ATTACH_PARENT_PROCESS,
      STD_OUTPUT_HANDLE,
    },
    SystemInformation::GetLocalTime,
  },
};

thread_local! {
  static CONSOLE_HANDLE: RefCell<Option<HANDLE>> = const { RefCell::new(None) };
}

pub fn enable_terminal_logging() {
  // SAFETY: both calls take only constants and return owned values. No
  // pointer owned by this process is dereferenced.
  let handle = unsafe {
    if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
      return;
    }
    let Ok(handle) = GetStdHandle(STD_OUTPUT_HANDLE) else {
      return;
    };
    handle
  };

  // The standard output handle cannot change while the process runs, so it
  // is validated once here rather than on every log line.
  if !handle.is_invalid() && !handle.0.is_null() {
    CONSOLE_HANDLE.with(|cell| *cell.borrow_mut() = Some(handle));
  }
}

pub fn console_log(args: Arguments<'_>) {
  CONSOLE_HANDLE.with(|cell| {
    // Nothing is attached, so there is nowhere to write. Returning here
    // keeps the timestamp and the UTF-16 buffer off every log call.
    let Some(handle) = *cell.borrow() else {
      return;
    };

    // SAFETY: `GetLocalTime` fills a value it owns and takes no pointers.
    let tm = unsafe { GetLocalTime() };
    let line = format!(
      "[{:02}:{:02}:{:02}] {}\r\n",
      tm.wHour, tm.wMinute, tm.wSecond, args
    );
    let wide: Vec<u16> = line.encode_utf16().collect();

    // SAFETY: `handle` was validated in `enable_terminal_logging` and the
    // process cannot outlive it, and both `line` and `wide` are live locals
    // for the duration of the calls.
    unsafe {
      if WriteConsoleW(handle, &wide, None, None).is_err() {
        let _ = WriteFile(handle, Some(line.as_bytes()), None, None);
      }
    }
  });
}

#[macro_export]
macro_rules! clog {
  ($($arg:tt)*) => {
    $crate::console::console_log(format_args!($($arg)*))
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn logging_without_a_console_is_a_no_op() {
    // The test binary never calls `enable_terminal_logging`, which is the
    // same state the GUI build runs in. Every log call takes this early
    // return, so a regression here has no other guard.
    console_log(format_args!("no console attached"));
  }
}
