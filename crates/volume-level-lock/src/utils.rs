#![cfg(windows)]

use windows::Win32::{
  Foundation::{LPARAM, WPARAM},
  UI::WindowsAndMessaging::PostThreadMessageW,
};

pub fn to_wide(s: &str) -> Vec<u16> {
  let mut w = Vec::with_capacity(s.len() + 1);
  w.extend(s.encode_utf16());
  w.push(0);
  w
}

pub fn wake_main_thread(main_thread_id: u32) {
  unsafe {
    let _ = PostThreadMessageW(
      main_thread_id,
      crate::WM_WAKEUP,
      WPARAM(0),
      LPARAM(0),
    );
  }
}
