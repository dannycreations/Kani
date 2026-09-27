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

const STRIP_STYLE_MASK: isize =
  !(WS_CAPTION.0 | WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0 | WS_MINIMIZEBOX.0)
    as isize;
const STRIP_EX_STYLE_MASK: isize =
  !(WS_EX_DLGMODALFRAME.0 | WS_EX_WINDOWEDGE.0 | WS_EX_CLIENTEDGE.0) as isize;

thread_local! {
  static APP_INSTANCE: RefCell<Option<App>> = const { RefCell::new(None) };
}

#[inline(always)]
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
  APP_INSTANCE.with(|cell| cell.borrow_mut().as_mut().map(f))
}

pub fn install(app: App) {
  APP_INSTANCE.with(|cell| *cell.borrow_mut() = Some(app));
}

#[derive(Debug)]
pub struct ManagedWindow {
  pub process_name: Rc<str>,
  pub title: Box<str>,
  pub original: OriginalWindowMetrics,
  last_seen: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct OriginalWindowMetrics {
  pub style: isize,
  pub ex_style: isize,
  pub rect: RECT,
  pub menu: HMENU,
}

#[inline(always)]
fn hwnd_to_key(hwnd: HWND) -> usize {
  hwnd.0 as usize
}

#[inline(always)]
fn key_to_hwnd(key: usize) -> HWND {
  HWND(key as *mut _)
}

#[inline(always)]
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

struct ProcessCacheEntry {
  name: Option<Rc<str>>,
  generation: u32,
}

#[derive(Default)]
struct ProcessNameCache {
  entries: FxHashMap<u32, ProcessCacheEntry>,
  generation: u32,
}

impl ProcessNameCache {
  #[inline]
  fn begin_scan(&mut self) -> u32 {
    self.generation = self.generation.wrapping_add(1);
    if (self.generation & 63) == 0 && self.entries.len() > 32 {
      let cur = self.generation;
      self
        .entries
        .retain(|_, v| cur.wrapping_sub(v.generation) < 64);
    }
    self.generation
  }

  #[inline]
  fn clear(&mut self) {
    self.entries.clear();
  }

  #[inline]
  fn get_or_query(&mut self, pid: u32, config: &Config) -> Option<Rc<str>> {
    if let Some(entry) = self.entries.get_mut(&pid) {
      entry.generation = self.generation;
      return entry.name.clone();
    }

    let monitored_name =
      get_process_name(pid).filter(|name| config.is_monitored(name));
    self.entries.insert(
      pid,
      ProcessCacheEntry {
        name: monitored_name.clone(),
        generation: self.generation,
      },
    );
    monitored_name
  }

  #[inline]
  fn get_name(&self, pid: u32) -> Option<Rc<str>> {
    self.entries.get(&pid).and_then(|e| e.name.clone())
  }
}

struct EnumContext<'a> {
  current_pid: u32,
  min_window_size: i32,
  generation: u32,
  last_pid: u32,
  last_monitored: Option<Rc<str>>,
  managed_windows: &'a mut FxHashMap<usize, ManagedWindow>,
  config: &'a Config,
  cache: &'a mut ProcessNameCache,
  newly_managed: Vec<(usize, Rc<str>, Box<str>, OriginalWindowMetrics)>,
}

unsafe extern "system" fn enum_windows_callback(
  hwnd: HWND,
  lparam: LPARAM,
) -> BOOL {
  let ctx = unsafe { &mut *(lparam.0 as *mut EnumContext) };

  let key = hwnd_to_key(hwnd);
  if let Some(existing) = ctx.managed_windows.get_mut(&key) {
    // Already managed and still enumerable: mark alive for this scan
    // generation so the post-scan prune can skip an `IsWindow` call.
    existing.last_seen = ctx.generation;
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
  // a process. Reuse the previous verdict for an identical pid instead of
  // re-entering the name cache.
  let name = if pid == ctx.last_pid {
    match &ctx.last_monitored {
      Some(n) => n.clone(),
      None => return BOOL(1),
    }
  } else {
    ctx.last_pid = pid;
    match ctx.cache.get_or_query(pid, ctx.config) {
      Some(n) => {
        ctx.last_monitored = Some(n.clone());
        n
      }
      None => {
        ctx.last_monitored = None;
        return BOOL(1);
      }
    }
  };

  let mut rect = RECT::default();
  if unsafe { GetWindowRect(hwnd, &mut rect).is_err() } {
    return BOOL(1);
  }

  let width = rect.right - rect.left;
  let height = rect.bottom - rect.top;
  if width < ctx.min_window_size || height < ctx.min_window_size {
    return BOOL(1);
  }

  let orig_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
  let has_caption = (orig_style as u32 & WS_CAPTION.0) != 0;
  let known_menu = if has_caption {
    None
  } else {
    let m = unsafe { GetMenu(hwnd) };
    if m.0.is_null() {
      return BOOL(1);
    }
    Some(m)
  };

  let title = get_window_title(hwnd);
  clog!("Found: {} \"{}\" ({}x{})", name, title, width, height);

  if let Some(metrics) =
    App::apply_borderless(hwnd, &name, known_menu, Some(orig_style), Some(rect))
  {
    ctx.newly_managed.push((key, name, title, metrics));
  }

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
  pub fn new(exe_path: PathBuf) -> Self {
    let config_path = exe_path.with_extension("ini");
    let config = Config::load_or_create(&config_path);
    let current_pid = unsafe { GetCurrentProcessId() };

    Self {
      exe_path,
      config_path,
      config,
      current_pid,
      main_hwnd: HWND::default(),
      overlay_hwnd: HWND::default(),
      autostart_enabled: is_autostart_enabled(),
      managed_windows: FxHashMap::default(),
      process_name_cache: ProcessNameCache::default(),
      tray_icon: None,
    }
  }

  #[inline(always)]
  pub fn managed_windows(&self) -> &FxHashMap<usize, ManagedWindow> {
    &self.managed_windows
  }

  pub fn initialize(&mut self, hwnd: HWND) {
    self.main_hwnd = hwnd;
    clog!("Monitoring: {}", self.config.process_names.join(", "));

    match TrayIconBuilder::new()
      .with_tooltip(APP_TITLE)
      .with_icon(tray::build_tray_icon())
      .build()
    {
      Ok(tray) => self.tray_icon = Some(tray),
      Err(err) => clog!("Failed to create tray icon: {err}"),
    }
    self.sync_tray();
    self.start_timer();
  }

  fn sync_tray(&mut self) {
    let Some(tray) = &self.tray_icon else { return };

    if self.managed_windows.is_empty() {
      let _ = tray.set_tooltip(Some(APP_TITLE));
    } else {
      let _ = tray.set_tooltip(Some(&format!(
        "{APP_TITLE} ({} active)",
        self.managed_windows.len()
      )));
    }

    let menu = tray::build_menu(self, self.autostart_enabled);
    tray.set_menu(Some(Box::new(menu)));
  }

  #[inline(always)]
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

  #[inline(always)]
  fn stop_timer(&self) {
    unsafe {
      let _ = KillTimer(Some(self.main_hwnd), TIMER_POLL_ID);
    }
  }

  #[inline]
  fn track_managed_window(
    &mut self,
    key: usize,
    process_name: Rc<str>,
    title: Box<str>,
    metrics: OriginalWindowMetrics,
    generation: u32,
  ) {
    self.managed_windows.insert(
      key,
      ManagedWindow {
        process_name,
        title,
        original: metrics,
        last_seen: generation,
      },
    );
  }

  pub fn check_monitored_windows(&mut self) {
    // Nothing configured to monitor. Fall back to a plain
    // `IsWindow`-based sweep since we won't be running `EnumWindows` below
    // to piggy-back liveness detection on.
    if self.config.process_names.is_empty() {
      let removed_any = self.prune_dead_managed_windows();
      if removed_any {
        self.sync_tray();
      }
      return;
    }

    let generation = self.process_name_cache.begin_scan();

    // Scoped so the context's borrows of `self` end before the prune and
    // track steps below, which both need `&mut self`.
    let newly_managed = {
      let mut context = EnumContext {
        current_pid: self.current_pid,
        min_window_size: self.config.min_window_size,
        generation,
        last_pid: 0,
        last_monitored: None,
        managed_windows: &mut self.managed_windows,
        config: &self.config,
        cache: &mut self.process_name_cache,
        newly_managed: Vec::new(),
      };

      unsafe {
        let ptr = &mut context as *mut EnumContext;
        let _ = EnumWindows(Some(enum_windows_callback), LPARAM(ptr as isize));
      }

      std::mem::take(&mut context.newly_managed)
    };

    let added_any = !newly_managed.is_empty();
    // Anything not touched by the enumeration above this generation is
    // either destroyed or otherwise unreachable; confirm with `IsWindow`
    // only for those stragglers instead of every managed window.
    let removed_any = self.prune_stale_managed_windows(generation);

    for (key, name, title, metrics) in newly_managed {
      self.track_managed_window(key, name, title, metrics, generation);
    }

    if added_any || removed_any {
      self.sync_tray();
    }
  }

  fn prune_dead_managed_windows(&mut self) -> bool {
    if self.managed_windows.is_empty() {
      return false;
    }
    let before = self.managed_windows.len();
    self
      .managed_windows
      .retain(|&key, _| is_valid_window(key_to_hwnd(key)));
    self.managed_windows.len() != before
  }

  fn prune_stale_managed_windows(&mut self, generation: u32) -> bool {
    if self.managed_windows.is_empty() {
      return false;
    }
    let before = self.managed_windows.len();
    self.managed_windows.retain(|&key, w| {
      w.last_seen == generation || is_valid_window(key_to_hwnd(key))
    });
    self.managed_windows.len() != before
  }

  pub fn apply_borderless(
    hwnd: HWND,
    process_name: &str,
    known_menu: Option<HMENU>,
    known_style: Option<isize>,
    known_rect: Option<RECT>,
  ) -> Option<OriginalWindowMetrics> {
    unsafe {
      let orig_style =
        known_style.unwrap_or_else(|| GetWindowLongPtrW(hwnd, GWL_STYLE));
      let orig_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
      let orig_rect = match known_rect {
        Some(r) => r,
        None => {
          let mut r = RECT::default();
          if GetWindowRect(hwnd, &mut r).is_err() {
            return None;
          }
          r
        }
      };

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

      let screen_x = screen_rect.left;
      let screen_y = screen_rect.top;
      let screen_w = screen_rect.right - screen_rect.left;
      let screen_h = screen_rect.bottom - screen_rect.top;

      let stripped_style = orig_style & STRIP_STYLE_MASK;
      if stripped_style != orig_style {
        SetWindowLongPtrW(hwnd, GWL_STYLE, stripped_style);
      }

      let stripped_ex_style = orig_ex_style & STRIP_EX_STYLE_MASK;
      if stripped_ex_style != orig_ex_style {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, stripped_ex_style);
      }

      let hmenu = known_menu.unwrap_or_else(|| GetMenu(hwnd));
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

      let mut target_rect = RECT {
        left: 0,
        top: 0,
        right: screen_w,
        bottom: screen_h,
      };

      let _ = AdjustWindowRectEx(
        &mut target_rect,
        WINDOW_STYLE(stripped_style as u32),
        false,
        WINDOW_EX_STYLE(stripped_ex_style as u32),
      );

      let final_x = screen_x + target_rect.left;
      let final_y = screen_y + target_rect.top;
      let final_w = target_rect.right - target_rect.left;
      let final_h = target_rect.bottom - target_rect.top;

      move_window_framed(hwnd, final_x, final_y, final_w, final_h);

      clog!(
        "Applied: {} \u{2192} {}x{}",
        process_name,
        screen_w,
        screen_h
      );

      Some(saved_metrics)
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
    self.process_name_cache.clear();
  }

  pub fn restore_borders(&mut self, hwnd: HWND) {
    let key = hwnd_to_key(hwnd);
    let Some(managed) = self.managed_windows.remove(&key) else {
      return;
    };

    if is_valid_window(hwnd) {
      Self::apply_restore_metrics(hwnd, &managed.original);
      clog!("Restored: {}", managed.title);
    }

    let still_managed = self
      .managed_windows
      .values()
      .any(|w| w.process_name.eq_ignore_ascii_case(&managed.process_name));
    if !still_managed && self.config.remove_process(&managed.process_name) {
      self.persist_config();
    }

    self.sync_tray();
  }

  pub fn restore_all_borders(&mut self) {
    if self.managed_windows.is_empty() {
      return;
    }

    for (key, managed) in self.managed_windows.drain() {
      let hwnd = key_to_hwnd(key);
      if is_valid_window(hwnd) {
        Self::apply_restore_metrics(hwnd, &managed.original);
        clog!("Restored: {}", managed.title);
      }
    }

    self.config.clear_processes();
    self.persist_config();

    self.sync_tray();
  }

  pub fn toggle_window(&mut self, hwnd: HWND) {
    let key = hwnd_to_key(hwnd);
    if self.managed_windows.contains_key(&key) {
      self.restore_borders(hwnd);
      return;
    }

    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let process_name = self
      .process_name_cache
      .get_name(pid)
      .or_else(|| get_process_name(pid))
      .unwrap_or_else(|| Rc::from("Unknown"));

    let Some(metrics) =
      Self::apply_borderless(hwnd, &process_name, None, None, None)
    else {
      return;
    };

    let title = get_window_title(hwnd);
    let generation = self.process_name_cache.generation;
    self.track_managed_window(
      key,
      process_name.clone(),
      title,
      metrics,
      generation,
    );

    // `add_process` already reports whether the list changed, so there is
    // no need to pre-check `is_monitored` and pay for a second lookup.
    if self.config.add_process((*process_name).to_owned()) {
      self.persist_config();
    }
    self.sync_tray();
  }

  pub fn start_window_picker(&mut self) {
    if is_valid_window(self.overlay_hwnd) {
      unsafe {
        let _ = DestroyWindow(self.overlay_hwnd);
      }
      self.overlay_hwnd = HWND::default();
    }

    if let Some(hwnd) = overlay::spawn() {
      self.overlay_hwnd = hwnd;
    }
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
        if let Some(hex) =
          other.strip_prefix(tray::MENU_ID_RESTORE_WINDOW_PREFIX)
        {
          if let Ok(key) = usize::from_str_radix(hex, 16) {
            self.restore_borders(key_to_hwnd(key));
          }
        }
      }
    }
  }

  pub fn exit_application(&mut self) {
    self.stop_timer();
    self.tray_icon = None;

    if is_valid_window(self.overlay_hwnd) {
      unsafe {
        let _ = DestroyWindow(self.overlay_hwnd);
      }
      self.overlay_hwnd = HWND::default();
    }

    unsafe {
      PostQuitMessage(0);
    }
  }
}
