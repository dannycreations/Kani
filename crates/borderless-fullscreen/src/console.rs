use std::{
  cell::RefCell,
  fmt::{Arguments, Write as _},
};

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

struct Logger {
  console: Option<HANDLE>,
  line: String,
  wide: Vec<u16>,
}

thread_local! {
  static LOGGER: RefCell<Logger> = const {
    RefCell::new(Logger {
      console: None,
      line: String::new(),
      wide: Vec::new(),
    })
  };
}

pub fn enable_terminal_logging() {
  if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
    return;
  }

  let Ok(handle) = (unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }) else {
    return;
  };
  // The standard output handle cannot change while the process runs, so it
  // is validated once here rather than on every log line.
  if !handle.is_invalid() && !handle.0.is_null() {
    LOGGER.with(|log| log.borrow_mut().console = Some(handle));
  }
}

pub fn console_log(args: Arguments<'_>) {
  LOGGER.with(|cell| {
    let b = &mut *cell.borrow_mut();
    let Some(handle) = b.console else {
      return;
    };

    let tm = unsafe { GetLocalTime() };

    b.line.clear();
    let _ = write!(
      b.line,
      "[{:02}:{:02}:{:02}] {}\r\n",
      tm.wHour, tm.wMinute, tm.wSecond, args
    );

    b.wide.clear();
    b.wide.reserve(b.line.len());
    if b.line.is_ascii() {
      for &byte in b.line.as_bytes() {
        b.wide.push(byte as u16);
      }
    } else {
      b.wide.extend(b.line.encode_utf16());
    }

    unsafe {
      if WriteConsoleW(handle, &b.wide, None, None).is_err() {
        let _ = WriteFile(handle, Some(b.line.as_bytes()), None, None);
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
