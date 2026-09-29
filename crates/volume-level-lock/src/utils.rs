#![cfg(windows)]

use windows::Win32::{
  Foundation::{LPARAM, WPARAM},
  UI::WindowsAndMessaging::PostThreadMessageW,
};

use crate::WM_WAKEUP;

pub fn to_wide(s: &str) -> Vec<u16> {
  let mut w = Vec::with_capacity(s.len() + 1);
  w.extend(s.encode_utf16());
  w.push(0);
  w
}

pub fn wake_main_thread(main_thread_id: u32) {
  // SAFETY: `PostThreadMessageW` only reads the thread id and posts a
  // message. The message loop owns the queue for the life of the
  // process, so the id stays valid.
  unsafe {
    let _ = PostThreadMessageW(main_thread_id, WM_WAKEUP, WPARAM(0), LPARAM(0));
  }
}
