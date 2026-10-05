use std::{cell::RefCell, path::PathBuf, rc::Rc};

use rustc_hash::FxHashMap;
use tray_icon::{TrayIcon, TrayIconBuilder};
use windows::{
  core::BOOL,
  Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
      GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    },
    System::Threading::GetCurrentProcessId,
    UI::WindowsAndMessaging::{
      AdjustWindowRectEx, DestroyWindow, DrawMenuBar, EnumWindows, GetMenu,
      GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
      GetWindowThreadProcessId, IsWindowVisible, KillTimer, PostQuitMessage,
      SetMenu, SetTimer, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
      GWL_STYLE, HMENU, SM_CXSCREEN, SM_CYSCREEN, SWP_FRAMECHANGED,
      SWP_NOOWNERZORDER, SWP_NOZORDER, WINDOW_EX_STYLE, WINDOW_STYLE,
      WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_WINDOWEDGE,
      WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_THICKFRAME,
    },
  },
};

use crate::{
  clog,
  config::Config,
  overlay,
  registry::{is_autostart_enabled, set_autostart_enabled},
  tray,
  utils::{
    get_process_name, get_window_title, is_valid_window, open_in_default_editor,
  },
};

const APP_TITLE: &str = "Borderless Fullscreen";
pub(crate) const TIMER_POLL_ID: usize = 1;

const MAX_CACHED_PROCESSES: usize = 256;

const STRIP_STYLE_MASK: isize =
  !(WS_CAPTION.0 | WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0 | WS_MINIMIZEBOX.0)
    as isize;
const STRIP_EX_STYLE_MASK: isize =
  !(WS_EX_DLGMODALFRAME.0 | WS_EX_WINDOWEDGE.0 | WS_EX_CLIENTEDGE.0) as isize;

thread_local! {
  static APP_INSTANCE: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
  APP_INSTANCE.with(|cell| cell.borrow_mut().as_mut().map(f))
}

pub fn install(app: App) {
  APP_INSTANCE.with(|cell| *cell.borrow_mut() = Some(app));
}

pub struct ManagedWindow {
  pub process_name: Rc<str>,
  pub title: Box<str>,
  original: OriginalWindowMetrics,
}

struct OriginalWindowMetrics {
  style: isize,
  ex_style: isize,
  rect: RECT,
  menu: HMENU,
}

fn hwnd_to_key(hwnd: HWND) -> usize {
  hwnd.0 as usize
}

fn key_to_hwnd(key: usize) -> HWND {
  HWND(key as *mut _)
}

fn move_window_framed(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
  unsafe {
    let _ = SetWindowPos(
      hwnd,
      Some(HWND::default()),
      x,
      y,
      w,
      h,
      SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOOWNERZORDER,
    );
  }
}

#[derive(Default)]
struct ProcessNameCache {
  entries: FxHashMap<u32, Option<Rc<str>>>,
}

impl ProcessNameCache {
  fn get_or_query(&mut self, pid: u32, config: &Config) -> Option<Rc<str>> {
    if let Some(cached) = self.entries.get(&pid) {
      return cached.clone();
    }

    // Bound the map by dropping it whole rather than aging entries out one
    // at a time. A clear costs one `OpenProcess` per live process, and a
    // desktop holds far fewer processes than this limit.
    if self.entries.len() >= MAX_CACHED_PROCESSES {
      self.entries.clear();
    }

    let name = get_process_name(pid).filter(|name| config.is_monitored(name));
    self.entries.insert(pid, name.clone());
    name
  }

  fn clear(&mut self) {
    self.entries.clear();
  }
}

struct EnumContext<'a> {
  current_pid: u32,
  min_window_size: i32,
  managed_windows: &'a mut FxHashMap<usize, ManagedWindow>,
  config: &'a Config,
  cache: &'a mut ProcessNameCache,
}

unsafe extern "system" fn enum_windows_callback(
  hwnd: HWND,
  lparam: LPARAM,
) -> BOOL {
  // SAFETY: `EnumWindows` passes back the `LPARAM` it was given for the
  // duration of the call, and `scan_windows` keeps the context alive and
  // exclusively borrowed for that whole call.
  let ctx = unsafe { &mut *(lparam.0 as *mut EnumContext) };

  let key = hwnd_to_key(hwnd);
  // Already managed, so its metrics must not be overwritten.
  if ctx.managed_windows.contains_key(&key) {
    return BOOL(1);
  }

  if !unsafe { IsWindowVisible(hwnd).as_bool() } {
    return BOOL(1);
  }

  let mut pid = 0u32;
  unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
  if pid == 0 || pid == ctx.current_pid {
    return BOOL(1);
  }

  // `EnumWindows` walks in z-order, so consecutive windows frequently share
  // a process. The cache already answers a repeat lookup without an
  // `OpenProcess` round trip, which is the only cost worth avoiding.
  let Some(name) = ctx.cache.get_or_query(pid, ctx.config) else {
    return BOOL(1);
  };

  // A window with neither a caption nor a menu is a popup or dialog
  // frame, not an application window worth maximizing.
  let orig_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
  if (orig_style as u32 & WS_CAPTION.0) == 0
    && unsafe { GetMenu(hwnd) }.0.is_null()
  {
    return BOOL(1);
  }

  let mut orig_rect = RECT::default();
  if unsafe { GetWindowRect(hwnd, &mut orig_rect).is_err() } {
    return BOOL(1);
  }

  let width = orig_rect.right - orig_rect.left;
  let height = orig_rect.bottom - orig_rect.top;
  if width < ctx.min_window_size || height < ctx.min_window_size {
    return BOOL(1);
  }

  let title = get_window_title(hwnd);
  clog!("Found: {} \"{}\" ({}x{})", name, title, width, height);

  let original = App::apply_borderless(hwnd, &name, orig_style, orig_rect);
  ctx.managed_windows.insert(
    key,
    ManagedWindow {
      process_name: name,
      title,
      original,
    },
  );

  BOOL(1)
}

pub struct App {
  exe_path: PathBuf,
  config_path: PathBuf,
  config: Config,
  current_pid: u32,
  main_hwnd: HWND,
  overlay_hwnd: HWND,
  autostart_enabled: bool,
  managed_windows: FxHashMap<usize, ManagedWindow>,
  process_name_cache: ProcessNameCache,
  tray_icon: Option<TrayIcon>,
}

impl App {
  pub fn new(exe_path: PathBuf, main_hwnd: HWND) -> Self {
    let config_path = exe_path.with_extension("ini");
    let config = Config::load_or_create(&config_path);
    let current_pid = unsafe { GetCurrentProcessId() };

    let mut app = Self {
      exe_path,
      config_path,
      config,
      current_pid,
      main_hwnd,
      overlay_hwnd: HWND::default(),
      autostart_enabled: is_autostart_enabled(),
      managed_windows: FxHashMap::default(),
      process_name_cache: ProcessNameCache::default(),
      tray_icon: None,
    };

    clog!("Monitoring: {}", app.config.process_names.join(", "));
    match TrayIconBuilder::new()
      .with_tooltip(APP_TITLE)
      .with_icon(tray::build_tray_icon())
      .build()
    {
      Ok(tray) => app.tray_icon = Some(tray),
      Err(err) => clog!("Failed to create tray icon: {err}"),
    }
    app.sync_tray();
    app.start_timer();
    app
  }

  fn sync_tray(&mut self) {
    let Some(tray) = &self.tray_icon else { return };

    let tooltip = if self.managed_windows.is_empty() {
      APP_TITLE.to_owned()
    } else {
      format!("{APP_TITLE} ({} active)", self.managed_windows.len())
    };
    let _ = tray.set_tooltip(Some(&tooltip));

    let menu = tray::build_menu(&self.managed_windows, self.autostart_enabled);
    tray.set_menu(Some(Box::new(menu)));
  }

  fn start_timer(&self) {
    unsafe {
      SetTimer(
        Some(self.main_hwnd),
        TIMER_POLL_ID,
        self.config.polling_interval_ms as u32,
        None,
      );
    }
  }

  fn stop_timer(&self) {
    unsafe {
      let _ = KillTimer(Some(self.main_hwnd), TIMER_POLL_ID);
    }
  }

  pub fn check_monitored_windows(&mut self) {
    // `sync_tray` rebuilds the whole menu, and this poll runs every couple
    // of seconds, so only do it when the active-window set actually moved.
    let mut changed = self.scan_windows();
    changed |= self.prune_dead_windows();
    if changed {
      self.sync_tray();
    }
  }

  fn scan_windows(&mut self) -> bool {
    // With nothing configured, the enumeration would reject every window
    // on the process-name lookup, so skip the walk entirely.
    if self.config.process_names.is_empty() {
      return false;
    }

    let before = self.managed_windows.len();

    // Scoped so the context's borrows of `self` end before the prune in
    // `check_monitored_windows`, which needs `&mut self`.
    {
      let mut context = EnumContext {
        current_pid: self.current_pid,
        min_window_size: self.config.min_window_size,
        managed_windows: &mut self.managed_windows,
        config: &self.config,
        cache: &mut self.process_name_cache,
      };

      // SAFETY: `context` is borrowed mutably for the whole call, the
      // callback only touches it through the pointer, and `EnumWindows`
      // returns before this scope ends.
      unsafe {
        let ptr = &mut context as *mut EnumContext;
        let _ = EnumWindows(Some(enum_windows_callback), LPARAM(ptr as isize));
      }
    }

    self.managed_windows.len() > before
  }

  fn prune_dead_windows(&mut self) -> bool {
    let before = self.managed_windows.len();
    self
      .managed_windows
      .retain(|&key, _| is_valid_window(key_to_hwnd(key)));
    self.managed_windows.len() != before
  }

  fn apply_borderless(
    hwnd: HWND,
    process_name: &str,
    orig_style: isize,
    orig_rect: RECT,
  ) -> OriginalWindowMetrics {
    unsafe {
      let orig_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
      let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
      let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
      };

      let screen_rect = if GetMonitorInfoW(monitor, &mut monitor_info).as_bool()
      {
        monitor_info.rcMonitor
      } else {
        RECT {
          left: 0,
          top: 0,
          right: GetSystemMetrics(SM_CXSCREEN),
          bottom: GetSystemMetrics(SM_CYSCREEN),
        }
      };

      let stripped_style = orig_style & STRIP_STYLE_MASK;
      if stripped_style != orig_style {
        SetWindowLongPtrW(hwnd, GWL_STYLE, stripped_style);
      }

      let stripped_ex_style = orig_ex_style & STRIP_EX_STYLE_MASK;
      if stripped_ex_style != orig_ex_style {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, stripped_ex_style);
      }

      let hmenu = GetMenu(hwnd);
      if !hmenu.0.is_null() {
        let _ = SetMenu(hwnd, Some(HMENU::default()));
        let _ = DrawMenuBar(hwnd);
      }

      let saved_metrics = OriginalWindowMetrics {
        style: orig_style,
        ex_style: orig_ex_style,
        rect: orig_rect,
        menu: hmenu,
      };

      // `AdjustWindowRectEx` grows a rect outward by the frame borders, so
      // feeding it the monitor rect in absolute coordinates yields the
      // borderless client area in absolute coordinates.
      let mut target_rect = screen_rect;
      let _ = AdjustWindowRectEx(
        &mut target_rect,
        WINDOW_STYLE(stripped_style as u32),
        false,
        WINDOW_EX_STYLE(stripped_ex_style as u32),
      );

      move_window_framed(
        hwnd,
        target_rect.left,
        target_rect.top,
        target_rect.right - target_rect.left,
        target_rect.bottom - target_rect.top,
      );

      clog!(
        "Applied: {} \u{2192} {}x{}",
        process_name,
        screen_rect.right - screen_rect.left,
        screen_rect.bottom - screen_rect.top
      );

      saved_metrics
    }
  }

  fn apply_restore_metrics(hwnd: HWND, saved: &OriginalWindowMetrics) {
    unsafe {
      SetWindowLongPtrW(hwnd, GWL_STYLE, saved.style);
      SetWindowLongPtrW(hwnd, GWL_EXSTYLE, saved.ex_style);

      if !saved.menu.0.is_null() {
        let _ = SetMenu(hwnd, Some(saved.menu));
        let _ = DrawMenuBar(hwnd);
      }
    }

    let orig_w = saved.rect.right - saved.rect.left;
    let orig_h = saved.rect.bottom - saved.rect.top;
    move_window_framed(hwnd, saved.rect.left, saved.rect.top, orig_w, orig_h);
  }

  fn persist_config(&mut self) {
    if let Err(err) = self.config.save(&self.config_path) {
      clog!("Failed to save config: {err}");
    }
    // Cached verdicts embed the monitored filter, so they are stale now.
    self.process_name_cache.clear();
  }

  fn restore_window(hwnd: HWND, managed: &ManagedWindow) {
    if !is_valid_window(hwnd) {
      return;
    }
    Self::apply_restore_metrics(hwnd, &managed.original);
    clog!("Restored: {}", managed.title);
  }

  fn restore_borders(&mut self, key: usize) {
    let Some(managed) = self.managed_windows.remove(&key) else {
      return;
    };

    Self::restore_window(key_to_hwnd(key), &managed);

    let still_managed = self
      .managed_windows
      .values()
      .any(|w| w.process_name.eq_ignore_ascii_case(&managed.process_name));
    if !still_managed && self.config.remove_process(&managed.process_name) {
      self.persist_config();
    }

    self.sync_tray();
  }

  fn restore_all_borders(&mut self) {
    if self.managed_windows.is_empty() {
      return;
    }

    for (key, managed) in self.managed_windows.drain() {
      Self::restore_window(key_to_hwnd(key), &managed);
    }

    self.config.process_names.clear();
    self.persist_config();

    self.sync_tray();
  }

  pub fn toggle_window(&mut self, hwnd: HWND) {
    let key = hwnd_to_key(hwnd);
    if self.managed_windows.contains_key(&key) {
      self.restore_borders(key);
      return;
    }

    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    // The cache only holds monitored processes, and this click is the
    // first look at a window the user picked, so query the process
    // directly instead of paying for a lookup that can miss.
    let process_name =
      get_process_name(pid).unwrap_or_else(|| Rc::from("Unknown"));

    let orig_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    let mut orig_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut orig_rect) }.is_err() {
      return;
    }

    let original =
      Self::apply_borderless(hwnd, &process_name, orig_style, orig_rect);
    self.managed_windows.insert(
      key,
      ManagedWindow {
        process_name: process_name.clone(),
        title: get_window_title(hwnd),
        original,
      },
    );

    // `add_process` already reports whether the list changed, so there is
    // no need to pre-check `is_monitored` and pay for a second lookup.
    if self.config.add_process(&process_name) {
      self.persist_config();
    }
    self.sync_tray();
  }

  fn destroy_overlay(&mut self) {
    if !is_valid_window(self.overlay_hwnd) {
      return;
    }
    unsafe {
      let _ = DestroyWindow(self.overlay_hwnd);
    }
    self.overlay_hwnd = HWND::default();
  }

  fn start_window_picker(&mut self) {
    self.destroy_overlay();
    self.overlay_hwnd = overlay::spawn().unwrap_or_default();
  }

  pub fn handle_menu_event(&mut self, id: &str) {
    match id {
      tray::MENU_ID_RESTORE_ALL => self.restore_all_borders(),
      tray::MENU_ID_PICK_WINDOW => self.start_window_picker(),
      tray::MENU_ID_SETTING_AUTOSTART => {
        self.autostart_enabled = !self.autostart_enabled;
        set_autostart_enabled(self.autostart_enabled, &self.exe_path);
        clog!("Auto-start registry updated: {}", self.autostart_enabled);
        self.sync_tray();
      }
      tray::MENU_ID_OPEN_CONFIG => {
        open_in_default_editor(&self.config_path);
      }
      tray::MENU_ID_EXIT => self.exit_application(),
      other => {
        // Restore rows carry the window key as hex after a fixed prefix;
        // anything else is not an id this build produces.
        let Some(key) = other
          .strip_prefix(tray::MENU_ID_RESTORE_WINDOW_PREFIX)
          .and_then(|hex| usize::from_str_radix(hex, 16).ok())
        else {
          return;
        };
        self.restore_borders(key);
      }
    }
  }

  pub fn exit_application(&mut self) {
    self.stop_timer();
    self.tray_icon = None;
    self.destroy_overlay();

    unsafe {
      PostQuitMessage(0);
    }
  }
}
