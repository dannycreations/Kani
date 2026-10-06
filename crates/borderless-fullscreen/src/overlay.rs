use std::{
  cell::Cell,
  ffi::c_void,
  sync::atomic::{AtomicPtr, Ordering},
};

use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::{
      COLORREF, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    },
    Graphics::Gdi::{
      CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject,
      GetStockObject, SetWindowRgn, BLACK_BRUSH, HBRUSH, HGDIOBJ, RGN_DIFF,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
      Input::KeyboardAndMouse::VK_ESCAPE,
      WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetAncestor,
        GetCursorPos, GetDesktopWindow, GetShellWindow, GetSystemMetrics,
        GetWindow, GetWindowRect, IsIconic, IsWindowVisible, LoadCursorW,
        RegisterClassExW, SetCursor, SetForegroundWindow,
        SetLayeredWindowAttributes, SetWindowPos, ShowWindow, WindowFromPoint,
        GA_ROOT, GW_HWNDNEXT, HCURSOR, HTTRANSPARENT, IDC_CROSS, LWA_ALPHA,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
        SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, WM_DESTROY, WM_KEYDOWN,
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_SETCURSOR, WNDCLASSEXW,
        WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
      },
    },
  },
};

use crate::{app::with_app, utils::is_valid_window};

const OVERLAY_WINDOW_CLASS: PCWSTR =
  w!("BorderlessFullscreen_OverlayWindowClass");
const HIGHLIGHT_WINDOW_CLASS: PCWSTR =
  w!("BorderlessFullscreen_HighlightWindowClass");

static CROSS_CURSOR_HANDLE: AtomicPtr<c_void> =
  AtomicPtr::new(std::ptr::null_mut());

#[derive(Clone, Copy)]
struct OverlayState {
  overlay: HWND,
  highlight: HWND,
  hovered: Option<(HWND, RECT)>,
  last_cursor: POINT,
  highlight_size: SIZE,
  desktop: HWND,
  shell: HWND,
}

impl Default for OverlayState {
  fn default() -> Self {
    Self {
      overlay: HWND::default(),
      highlight: HWND::default(),
      hovered: None,
      last_cursor: POINT {
        x: i32::MIN,
        y: i32::MIN,
      },
      highlight_size: SIZE::default(),
      desktop: HWND::default(),
      shell: HWND::default(),
    }
  }
}

thread_local! {
  static STATE: Cell<OverlayState> = Cell::new(OverlayState::default());
}

fn with_state<R>(f: impl FnOnce(&mut OverlayState) -> R) -> R {
  STATE.with(|cell| {
    let mut state = cell.get();
    let result = f(&mut state);
    cell.set(state);
    result
  })
}

fn point_in_rect(pt: POINT, r: RECT) -> bool {
  pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom
}

pub fn register_class(hinstance: HMODULE) {
  unsafe {
    let cross = LoadCursorW(None, IDC_CROSS).unwrap_or_default();
    CROSS_CURSOR_HANDLE.store(cross.0, Ordering::Relaxed);

    let wc = WNDCLASSEXW {
      cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
      lpfnWndProc: Some(overlay_window_proc),
      hInstance: hinstance.into(),
      hCursor: cross,
      hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
      lpszClassName: OVERLAY_WINDOW_CLASS,
      ..Default::default()
    };
    RegisterClassExW(&wc);

    let highlight_brush = CreateSolidBrush(COLORREF(0x0000_FFFF));
    let wc2 = WNDCLASSEXW {
      cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
      lpfnWndProc: Some(highlight_window_proc),
      hInstance: hinstance.into(),
      hCursor: cross,
      hbrBackground: highlight_brush,
      lpszClassName: HIGHLIGHT_WINDOW_CLASS,
      ..Default::default()
    };
    RegisterClassExW(&wc2);
  }
}

pub fn spawn() -> Option<HWND> {
  with_state(|s| *s = OverlayState::default());

  unsafe {
    let hinstance = GetModuleHandleW(None).unwrap_or_default();
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

    let hwnd = CreateWindowExW(
      WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
      OVERLAY_WINDOW_CLASS,
      w!("BorderlessFullscreen_Overlay"),
      WS_POPUP,
      x,
      y,
      w,
      h,
      Some(HWND::default()),
      None,
      Some(hinstance.into()),
      None,
    )
    .ok()?;

    with_state(|s| {
      s.overlay = hwnd;
      s.desktop = GetDesktopWindow();
      s.shell = GetShellWindow();
    });

    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    Some(hwnd)
  }
}

unsafe fn position_highlight(hwnd: HWND, rect: RECT, last_size: SIZE) -> SIZE {
  // A window narrower or shorter than one border leaves no hollow
  // interior to outline, so there is nothing to draw.
  const THICKNESS: i32 = 4;
  let width = rect.right - rect.left;
  let height = rect.bottom - rect.top;
  if width <= THICKNESS * 2 || height <= THICKNESS * 2 {
    return last_size;
  }

  let mut size = last_size;
  if size.cx != width || size.cy != height {
    let outer = CreateRectRgn(0, 0, width, height);
    let inner = CreateRectRgn(
      THICKNESS,
      THICKNESS,
      width - THICKNESS,
      height - THICKNESS,
    );
    let region = CreateRectRgn(0, 0, 1, 1);
    let _ = CombineRgn(Some(region), Some(outer), Some(inner), RGN_DIFF);
    let _ = DeleteObject(HGDIOBJ(outer.0));
    let _ = DeleteObject(HGDIOBJ(inner.0));
    let _ = SetWindowRgn(hwnd, Some(region), true);
    size = SIZE {
      cx: width,
      cy: height,
    };
  }

  let _ = SetWindowPos(
    hwnd,
    Some(HWND::default()),
    rect.left,
    rect.top,
    width,
    height,
    SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
  );
  size
}

unsafe fn create_highlight_window(rect: RECT) -> Option<HWND> {
  let width = rect.right - rect.left;
  let height = rect.bottom - rect.top;
  if width <= 0 || height <= 0 {
    return None;
  }

  let hinstance = GetModuleHandleW(None).unwrap_or_default();
  let hwnd = CreateWindowExW(
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
    HIGHLIGHT_WINDOW_CLASS,
    w!("BorderlessFullscreen_Highlight"),
    WS_POPUP,
    rect.left,
    rect.top,
    width,
    height,
    Some(HWND::default()),
    None,
    Some(hinstance.into()),
    None,
  )
  .ok()?;

  let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
  Some(hwnd)
}

fn find_window_under_point(
  pt: POINT,
  state: &OverlayState,
  hovered: Option<(HWND, RECT)>,
) -> Option<(HWND, RECT)> {
  // Below this size a click is far more likely to be aimed at a toolbar
  // control than at a window the user wants maximized.
  const MIN_PICKABLE_SIZE: i32 = 32;

  unsafe {
    let hit = WindowFromPoint(pt);
    if hit == HWND::default() {
      return None;
    }

    let mut root = GetAncestor(hit, GA_ROOT);
    if root == HWND::default() {
      root = hit;
    }

    while root != HWND::default() {
      // Only revalidate the previously hovered window once the walk actually
      // reaches it, so the three Win32 calls below are skipped entirely when
      // the cursor has moved onto a different window.
      if let Some((window, rect)) = hovered {
        if root == window
          && point_in_rect(pt, rect)
          && is_valid_window(window)
          && IsWindowVisible(window).as_bool()
          && !IsIconic(window).as_bool()
        {
          return Some((window, rect));
        }
      }

      if root != state.overlay
        && root != state.highlight
        && root != state.desktop
        && root != state.shell
        && IsWindowVisible(root).as_bool()
        && !IsIconic(root).as_bool()
      {
        let mut rect = RECT::default();
        if GetWindowRect(root, &mut rect).is_ok()
          && (rect.right - rect.left) >= MIN_PICKABLE_SIZE
          && (rect.bottom - rect.top) >= MIN_PICKABLE_SIZE
          && point_in_rect(pt, rect)
        {
          return Some((root, rect));
        }
      }
      root = match GetWindow(root, GW_HWNDNEXT) {
        Ok(next) => next,
        Err(_) => break,
      };
    }

    None
  }
}

unsafe extern "system" fn overlay_window_proc(
  hwnd: HWND,
  msg: u32,
  wparam: WPARAM,
  lparam: LPARAM,
) -> LRESULT {
  match msg {
    WM_SETCURSOR => {
      let cursor = CROSS_CURSOR_HANDLE.load(Ordering::Relaxed);
      SetCursor(Some(HCURSOR(cursor)));
      LRESULT(1)
    }
    WM_KEYDOWN => {
      if wparam.0 == VK_ESCAPE.0 as usize {
        let _ = DestroyWindow(hwnd);
      }
      LRESULT(0)
    }
    WM_MOUSEMOVE => {
      let mut pt = POINT::default();
      let _ = GetCursorPos(&mut pt);

      with_state(|s| {
        if s.last_cursor.x == pt.x && s.last_cursor.y == pt.y {
          return;
        }
        s.last_cursor = pt;

        match find_window_under_point(pt, s, s.hovered) {
          Some((root, rect)) => {
            if s.hovered == Some((root, rect)) && is_valid_window(s.highlight) {
              return;
            }

            let highlight = if is_valid_window(s.highlight) {
              Some(s.highlight)
            } else {
              create_highlight_window(rect)
            };
            if let Some(hwnd) = highlight {
              s.highlight_size =
                position_highlight(hwnd, rect, s.highlight_size);
              let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
              s.highlight = hwnd;
              s.hovered = Some((root, rect));
            }
          }
          None => {
            if is_valid_window(s.highlight) {
              let _ = ShowWindow(s.highlight, SW_HIDE);
            }
            s.hovered = None;
          }
        }
      });
      LRESULT(0)
    }
    WM_LBUTTONUP => {
      let target = with_state(|s| {
        let target = match s.hovered {
          Some((window, _)) if is_valid_window(window) => Some(window),
          // Re-probe with no cache: a stale `hovered` entry would let the
          // walk return a window the user did not click.
          _ => {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            find_window_under_point(pt, s, None).map(|(window, _)| window)
          }
        };

        if is_valid_window(s.highlight) {
          let _ = DestroyWindow(s.highlight);
        }
        if is_valid_window(s.overlay) {
          let _ = DestroyWindow(s.overlay);
        }
        *s = OverlayState::default();
        target
      });

      if let Some(target) = target {
        with_app(|app| app.toggle_window(target));
      }
      LRESULT(0)
    }
    WM_DESTROY => {
      with_state(|s| {
        if is_valid_window(s.highlight) {
          let _ = DestroyWindow(s.highlight);
        }
        *s = OverlayState::default();
      });
      LRESULT(0)
    }
    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
  }
}

unsafe extern "system" fn highlight_window_proc(
  hwnd: HWND,
  msg: u32,
  wparam: WPARAM,
  lparam: LPARAM,
) -> LRESULT {
  match msg {
    WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
    WM_DESTROY => {
      with_state(|s| {
        if s.highlight == hwnd {
          s.highlight = HWND::default();
          s.hovered = None;
          s.highlight_size = SIZE::default();
        }
      });
      LRESULT(0)
    }
    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
  }
}
