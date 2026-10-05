#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod config;
mod console;
mod overlay;
mod registry;
mod tray;
mod utils;

use std::{
  ffi::c_void,
  sync::atomic::{AtomicPtr, Ordering},
};

use tray_icon::menu::MenuEvent;
use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::{
      GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
    },
    System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
    UI::WindowsAndMessaging::{
      CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
      MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW,
      TranslateMessage, MB_ICONINFORMATION, MB_OK, MSG, WM_CLOSE, WM_DESTROY,
      WM_TIMER, WNDCLASSEXW, WS_OVERLAPPED,
    },
  },
};

use crate::{
  app::{with_app, App, TIMER_POLL_ID},
  console::enable_terminal_logging,
};

const SINGLE_INSTANCE_MUTEX: PCWSTR = w!("BorderlessFullscreen_SingleInstance");
const MAIN_WINDOW_CLASS: PCWSTR = w!("BorderlessFullscreen_MessageWindowClass");

static MAIN_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "system" fn main_window_proc(
  hwnd: HWND,
  msg: u32,
  wparam: WPARAM,
  lparam: LPARAM,
) -> LRESULT {
  match msg {
    WM_TIMER => {
      if wparam.0 == TIMER_POLL_ID {
        with_app(|app| app.check_monitored_windows());
      }
      LRESULT(0)
    }
    WM_CLOSE => {
      with_app(|app| app.exit_application());
      LRESULT(0)
    }
    WM_DESTROY => {
      PostQuitMessage(0);
      LRESULT(0)
    }
    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
  }
}

fn main() -> windows::core::Result<()> {
  enable_terminal_logging();

  // The handle must stay open for the life of the process. Closing
  // the last handle to a named mutex destroys it and frees the name, which
  // would let a second instance claim it.
  let _single_instance =
    unsafe { CreateMutexW(None, true, SINGLE_INSTANCE_MUTEX)? };
  if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
    unsafe {
      MessageBoxW(
        Some(HWND::default()),
        w!("Borderless Fullscreen is already running.\nCheck the system tray."),
        w!("Borderless Fullscreen"),
        MB_OK | MB_ICONINFORMATION,
      );
    }
    return Ok(());
  }

  // The config path and the autostart entry are both derived from the
  // executable's own location, so a wrong path here would be written to
  // disk and to the registry. Fail startup instead of guessing.
  let exe_path = std::env::current_exe()?;

  // SAFETY: startup runs on the process's only thread, the class names
  // are unique to this binary, and the single-instance mutex above rules
  // out a second registration.
  let main_hwnd = unsafe {
    let hinstance = GetModuleHandleW(None).unwrap_or_default();

    // Never shown and never painted, so it carries no redraw styles.
    let main_wc = WNDCLASSEXW {
      cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
      lpfnWndProc: Some(main_window_proc),
      hInstance: hinstance.into(),
      lpszClassName: MAIN_WINDOW_CLASS,
      ..Default::default()
    };
    RegisterClassExW(&main_wc);

    overlay::register_class(hinstance);

    CreateWindowExW(
      Default::default(),
      MAIN_WINDOW_CLASS,
      w!("BorderlessFullscreen_HiddenDispatcher"),
      WS_OVERLAPPED,
      0,
      0,
      0,
      0,
      Some(HWND::default()),
      None,
      Some(hinstance.into()),
      None,
    )?
  };

  MAIN_HWND.store(main_hwnd.0, Ordering::Relaxed);

  let _ = ctrlc::set_handler(|| {
    let hwnd = MAIN_HWND.load(Ordering::Relaxed);
    if !hwnd.is_null() {
      // SAFETY: `PostMessageW` only reads the handle and posts a
      // message. The dispatcher window outlives every thread in the
      // process, so the handle stays valid for the whole run.
      let _ = unsafe {
        PostMessageW(Some(HWND(hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0))
      };
    }
  });

  app::install(App::new(exe_path, main_hwnd));

  let mut msg = MSG::default();
  let menu_events = MenuEvent::receiver();
  loop {
    // SAFETY: `msg` is a live `MSG` and the dispatcher window created
    // above is never destroyed before the loop ends.
    let status = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
    // A -1 return means the call failed, and treating it as a message
    // would spin forever, so stop on anything that is not one.
    if status <= 0 {
      break;
    }

    // SAFETY: dispatching `msg` can only re-enter the window procedures
    // this process registered.
    unsafe {
      let _ = TranslateMessage(&msg);
      DispatchMessageW(&msg);
    }

    while let Ok(event) = menu_events.try_recv() {
      with_app(|app| app.handle_menu_event(&event.id.0));
    }
  }

  Ok(())
}
