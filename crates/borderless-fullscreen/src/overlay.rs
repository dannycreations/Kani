use std::{
  cell::Cell,
  sync::atomic::{AtomicUsize, Ordering},
};

use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::{
      COLORREF, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
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

static CROSS_CURSOR_HANDLE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct OverlayState {
  overlay: HWND,
  highlight: HWND,
  hovered: HWND,
  hovered_rect: RECT,
  last_cursor: POINT,
  highlight_size: (i32, i32),
  desktop: HWND,
  shell: HWND,
}

impl Default for OverlayState {
  fn default() -> Self {
    Self {
      overlay: HWND::default(),
      highlight: HWND::default(),
      hovered: HWND::default(),
      hovered_rect: RECT::default(),
      last_cursor: POINT {
        x: i32::MIN,
        y: i32::MIN,
      },
      highlight_size: (0, 0),
      desktop: HWND::default(),
      shell: HWND::default(),
    }
  }
}

thread_local! {
  static STATE: Cell<OverlayState> = Cell::new(OverlayState::default());
}

#[inline(always)]
fn point_in_rect(pt: POINT, r: RECT) -> bool {
  pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom
}

pub fn register_class(hinstance: HMODULE) {
  unsafe {
    let cross = LoadCursorW(None, IDC_CROSS).unwrap_or_default();
    CROSS_CURSOR_HANDLE.store(cross.0 as usize, Ordering::Relaxed);

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
  STATE.with(|c| c.set(OverlayState::default()));

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

    let desktop = GetDesktopWindow();
    let shell = GetShellWindow();

    STATE.with(|c| {
      let mut s = c.get();
      s.overlay = hwnd;
      s.desktop = desktop;
      s.shell = shell;
      c.set(s);
    });

    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    Some(hwnd)
  }
}

unsafe fn position_highlight(
  hwnd: HWND,
  rect: RECT,
  last_size: &mut (i32, i32),
) {
  let width = rect.right - rect.left;
  let height = rect.bottom - rect.top;
  if width <= 0 || height <= 0 {
    return;
  }

  const THICKNESS: i32 = 4;
  if width <= THICKNESS * 2 || height <= THICKNESS * 2 {
    return;
  }

  if last_size.0 != width || last_size.1 != height {
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
    *last_size = (width, height);
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
}

unsafe fn create_highlight_window(
  rect: RECT,
  last_size: &mut (i32, i32),
) -> Option<HWND> {
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
  position_highlight(hwnd, rect, last_size);
  let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
  Some(hwnd)
}

fn find_window_under_point(
  pt: POINT,
  overlay: HWND,
  highlight: HWND,
  desktop: HWND,
  shell: HWND,
  cached_hovered: HWND,
  cached_rect: RECT,
) -> Option<(HWND, RECT)> {
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
      if root == cached_hovered
        && point_in_rect(pt, cached_rect)
        && is_valid_window(cached_hovered)
        && IsWindowVisible(cached_hovered).as_bool()
        && !IsIconic(cached_hovered).as_bool()
      {
        return Some((cached_hovered, cached_rect));
      }

      if root != overlay
        && root != highlight
        && root != desktop
        && root != shell
        && IsWindowVisible(root).as_bool()
        && !IsIconic(root).as_bool()
      {
        let mut rect = RECT::default();
        if GetWindowRect(root, &mut rect).is_ok()
          && (rect.right - rect.left) >= 32
          && (rect.bottom - rect.top) >= 32
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
      let raw = CROSS_CURSOR_HANDLE.load(Ordering::Relaxed);
      SetCursor(Some(HCURSOR(raw as *mut _)));
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

      let mut s = STATE.with(|c| c.get());
      if s.last_cursor.x == pt.x && s.last_cursor.y == pt.y {
        return LRESULT(0);
      }
      s.last_cursor = pt;

      let target = find_window_under_point(
        pt,
        s.overlay,
        s.highlight,
        s.desktop,
        s.shell,
        s.hovered,
        s.hovered_rect,
      );

      match target {
        Some((root, rect)) => {
          if root == s.hovered
            && rect == s.hovered_rect
            && is_valid_window(s.highlight)
          {
            STATE.with(|c| c.set(s));
            return LRESULT(0);
          }

          let highlight_hwnd = if is_valid_window(s.highlight) {
            position_highlight(s.highlight, rect, &mut s.highlight_size);
            let _ = ShowWindow(s.highlight, SW_SHOWNOACTIVATE);
            s.highlight
          } else {
            create_highlight_window(rect, &mut s.highlight_size)
              .unwrap_or_default()
          };

          if is_valid_window(highlight_hwnd) {
            s.highlight = highlight_hwnd;
            s.hovered = root;
            s.hovered_rect = rect;
          }
        }
        None => {
          if is_valid_window(s.highlight) {
            let _ = ShowWindow(s.highlight, SW_HIDE);
          }
          s.hovered = HWND::default();
          s.hovered_rect = RECT::default();
        }
      }

      STATE.with(|c| c.set(s));
      LRESULT(0)
    }
    WM_LBUTTONUP => {
      let s = STATE.with(|c| c.get());
      let target_window = if is_valid_window(s.hovered) {
        Some(s.hovered)
      } else {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        find_window_under_point(
          pt,
          s.overlay,
          s.highlight,
          s.desktop,
          s.shell,
          HWND::default(),
          RECT::default(),
        )
        .map(|(w, _)| w)
      };

      if is_valid_window(s.highlight) {
        let _ = DestroyWindow(s.highlight);
      }
      let overlay = s.overlay;
      STATE.with(|c| c.set(OverlayState::default()));

      if is_valid_window(overlay) {
        let _ = DestroyWindow(overlay);
      }

      if let Some(target) = target_window {
        with_app(|app| app.toggle_window(target));
      }
      LRESULT(0)
    }
    WM_DESTROY => {
      let s = STATE.with(|c| c.get());
      if is_valid_window(s.highlight) {
        let _ = DestroyWindow(s.highlight);
      }
      STATE.with(|c| c.set(OverlayState::default()));
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
      STATE.with(|c| {
        let mut s = c.get();
        if s.highlight == hwnd {
          s.highlight = HWND::default();
          s.hovered = HWND::default();
          s.hovered_rect = RECT::default();
          s.highlight_size = (0, 0);
          c.set(s);
        }
      });
      LRESULT(0)
    }
    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
  }
}
