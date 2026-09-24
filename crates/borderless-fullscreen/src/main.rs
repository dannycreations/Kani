#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod config;
mod overlay;
mod registry;
mod tray;
mod utils;

use std::{
  path::PathBuf,
  sync::atomic::{AtomicUsize, Ordering},
};

use tray_icon::menu::MenuEvent;
use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::{
      GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
    },
    System::{
      Console::{AttachConsole, ATTACH_PARENT_PROCESS},
      LibraryLoader::GetModuleHandleW,
      Threading::CreateMutexW,
    },
    UI::WindowsAndMessaging::{
      CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
      MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW,
      TranslateMessage, CS_HREDRAW, CS_VREDRAW, MB_ICONINFORMATION, MB_OK, MSG,
      WM_CLOSE, WM_DESTROY, WM_TIMER, WNDCLASSEXW, WS_OVERLAPPED,
    },
  },
};

use crate::app::{with_app, App, TIMER_POLL_ID};

const SINGLE_INSTANCE_MUTEX: PCWSTR = w!("BorderlessFullscreen_SingleInstance");
const MAIN_WINDOW_CLASS: PCWSTR = w!("BorderlessFullscreen_MessageWindowClass");

static MAIN_HWND: AtomicUsize = AtomicUsize::new(0);

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
  unsafe {
    let _ = AttachConsole(ATTACH_PARENT_PROCESS);

    let _mutex = CreateMutexW(None, true, SINGLE_INSTANCE_MUTEX)?;
    if GetLastError() == ERROR_ALREADY_EXISTS {
      MessageBoxW(
        Some(HWND::default()),
        w!("Borderless Fullscreen is already running.\nCheck the system tray."),
        w!("Borderless Fullscreen"),
        MB_OK | MB_ICONINFORMATION,
      );
      return Ok(());
    }
  }

  let exe_path = std::env::current_exe()
    .unwrap_or_else(|_| PathBuf::from("borderless_fullscreen.exe"));

  unsafe {
    let hinstance = GetModuleHandleW(None).unwrap_or_default();

    let main_wc = WNDCLASSEXW {
      cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
      style: CS_HREDRAW | CS_VREDRAW,
      lpfnWndProc: Some(main_window_proc),
      hInstance: hinstance.into(),
      lpszClassName: MAIN_WINDOW_CLASS,
      ..Default::default()
    };
    RegisterClassExW(&main_wc);

    overlay::register_class(hinstance);

    let main_hwnd = CreateWindowExW(
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
    )?;

    MAIN_HWND.store(main_hwnd.0 as usize, Ordering::Relaxed);

    let _ = ctrlc::set_handler(|| {
      let raw = MAIN_HWND.load(Ordering::Relaxed);
      if raw != 0 {
        let _ = PostMessageW(
          Some(HWND(raw as *mut _)),
          WM_CLOSE,
          WPARAM(0),
          LPARAM(0),
        );
      }
    });

    let mut app = App::new(exe_path);
    app.initialize(main_hwnd);
    app::install(app);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
      let _ = TranslateMessage(&msg);
      DispatchMessageW(&msg);

      while let Ok(event) = MenuEvent::receiver().try_recv() {
        with_app(|app| app.handle_menu_event(&event.id.0));
      }
    }
  }

  Ok(())
}
