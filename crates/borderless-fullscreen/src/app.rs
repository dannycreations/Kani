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
    get_process_name, get_window_title, is_valid_window,
    open_in_default_editor, set_console_visibility,
  },
};

pub const APP_TITLE: &str = "Borderless Fullscreen";
pub(crate) const TIMER_POLL_ID: usize = 1;

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

#[derive(Debug, Clone)]
pub struct ManagedWindow {
  pub process_name: Rc<str>,
  pub title: Box<str>,
  pub original: OriginalWindowMetrics,
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

struct ProcessCacheEntry {
  name: Option<Rc<str>>,
  monitored: bool,
  generation: u32,
}

#[derive(Default)]
struct ProcessNameCache {
  entries: FxHashMap<u32, ProcessCacheEntry>,
  generation: u32,
}

impl ProcessNameCache {
  #[inline]
  fn begin_scan(&mut self) {
    self.generation = self.generation.wrapping_add(1);
    if (self.generation & 63) == 0 && self.entries.len() > 32 {
      let cur = self.generation;
      self
        .entries
        .retain(|_, v| cur.wrapping_sub(v.generation) < 64);
    }
  }

  fn recheck_monitored(&mut self, config: &Config) {
    for entry in self.entries.values_mut() {
      if let Some(name) = &entry.name {
        entry.monitored = config.is_monitored(name);
      }
    }
  }

  #[inline]
  fn query_monitored(&mut self, pid: u32, config: &Config) -> Option<Rc<str>> {
    if let Some(entry) = self.entries.get_mut(&pid) {
      entry.generation = self.generation;
      return if entry.monitored {
        entry.name.clone()
      } else {
        None
      };
    }

    let (name, monitored) = match get_process_name(pid) {
      Some(name) => {
        let is_mon = config.is_monitored(&name);
        (Some(name), is_mon)
      }
      None => (None, false),
    };

    let result = if monitored { name.clone() } else { None };
    self.entries.insert(
      pid,
      ProcessCacheEntry {
        name,
        monitored,
        generation: self.generation,
      },
    );
    result
  }

  fn get_name(&self, pid: u32) -> Option<Rc<str>> {
    self.entries.get(&pid).and_then(|e| e.name.clone())
  }
}

pub struct App {
  exe_path: PathBuf,
  config_path: PathBuf,
  config: Config,
  main_hwnd: HWND,
  overlay_hwnd: HWND,
  console_visible: bool,
  autostart_enabled: bool,
  managed_windows: FxHashMap<usize, ManagedWindow>,
  process_name_cache: ProcessNameCache,
  tray_icon: Option<TrayIcon>,
}

impl App {
  pub fn new(exe_path: PathBuf) -> Self {
    let config_path = exe_path.with_extension("ini");
    let config = Config::load_or_create(&config_path);

    Self {
      exe_path,
      config_path,
      config,
      main_hwnd: HWND::default(),
      overlay_hwnd: HWND::default(),
      console_visible: false,
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

  #[inline(always)]
  pub fn console_visible(&self) -> bool {
    self.console_visible
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
  ) {
    self.managed_windows.insert(
      key,
      ManagedWindow {
        process_name,
        title,
        original: metrics,
      },
    );
  }

  pub fn check_monitored_windows(&mut self) {
    let removed_any = self.prune_dead_managed_windows();

    if self.config.process_names.is_empty() {
      if removed_any {
        self.sync_tray();
      }
      return;
    }

    self.process_name_cache.begin_scan();

    struct EnumContext<'a> {
      min_window_size: i32,
      managed_windows: &'a FxHashMap<usize, ManagedWindow>,
      config: &'a Config,
      cache: &'a mut ProcessNameCache,
      newly_managed: Vec<(usize, Rc<str>, Box<str>, OriginalWindowMetrics)>,
    }

    unsafe extern "system" fn enum_windows_callback(
      hwnd: HWND,
      lparam: LPARAM,
    ) -> BOOL {
      let ctx = unsafe { &mut *(lparam.0 as *mut EnumContext) };

      if !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return BOOL(1);
      }

      let key = hwnd_to_key(hwnd);
      if ctx.managed_windows.contains_key(&key) {
        return BOOL(1);
      }

      let mut pid = 0u32;
      unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
      if pid == 0 {
        return BOOL(1);
      }

      let Some(name) = ctx.cache.query_monitored(pid, ctx.config) else {
        return BOOL(1);
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

      let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
      let has_caption = (style & WS_CAPTION.0) != 0;
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

      if let Some(metrics) = App::apply_borderless(hwnd, &name, known_menu) {
        ctx.newly_managed.push((key, name, title, metrics));
      }

      BOOL(1)
    }

    let mut context = EnumContext {
      min_window_size: self.config.min_window_size,
      managed_windows: &self.managed_windows,
      config: &self.config,
      cache: &mut self.process_name_cache,
      newly_managed: Vec::new(),
    };

    unsafe {
      let ptr = &mut context as *mut EnumContext;
      let _ = EnumWindows(Some(enum_windows_callback), LPARAM(ptr as isize));
    }

    let added_any = !context.newly_managed.is_empty();
    for (key, name, title, metrics) in context.newly_managed {
      self.track_managed_window(key, name, title, metrics);
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

  pub fn apply_borderless(
    hwnd: HWND,
    process_name: &str,
    known_menu: Option<HMENU>,
  ) -> Option<OriginalWindowMetrics> {
    unsafe {
      let orig_style = GetWindowLongPtrW(hwnd, GWL_STYLE);
      let orig_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
      let mut orig_rect = RECT::default();
      if GetWindowRect(hwnd, &mut orig_rect).is_err() {
        return None;
      }

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

      let stripped_style = orig_style
        & !(WS_CAPTION.0
          | WS_THICKFRAME.0
          | WS_MAXIMIZEBOX.0
          | WS_MINIMIZEBOX.0) as isize;
      if stripped_style != orig_style {
        SetWindowLongPtrW(hwnd, GWL_STYLE, stripped_style);
      }

      let stripped_ex_style = orig_ex_style
        & !(WS_EX_DLGMODALFRAME.0 | WS_EX_WINDOWEDGE.0 | WS_EX_CLIENTEDGE.0)
          as isize;
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

      let _ = SetWindowPos(
        hwnd,
        Some(HWND::default()),
        final_x,
        final_y,
        final_w,
        final_h,
        SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOOWNERZORDER,
      );

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

      let orig_w = saved.rect.right - saved.rect.left;
      let orig_h = saved.rect.bottom - saved.rect.top;

      let _ = SetWindowPos(
        hwnd,
        Some(HWND::default()),
        saved.rect.left,
        saved.rect.top,
        orig_w,
        orig_h,
        SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOOWNERZORDER,
      );
    }
  }

  pub fn restore_borders(&mut self, hwnd: HWND) {
    let key = hwnd_to_key(hwnd);
    let Some(managed) = self.managed_windows.remove(&key) else {
      return;
    };

    Self::apply_restore_metrics(hwnd, &managed.original);
    clog!("Restored: {}", managed.title);

    let still_managed = self
      .managed_windows
      .values()
      .any(|w| w.process_name.eq_ignore_ascii_case(&managed.process_name));
    if !still_managed && self.config.remove_process(&managed.process_name) {
      let _ = self.config.save(&self.config_path);
      self.process_name_cache.recheck_monitored(&self.config);
    }

    self.sync_tray();
  }

  pub fn restore_all_borders(&mut self) {
    if self.managed_windows.is_empty() {
      return;
    }

    for (key, managed) in self.managed_windows.drain() {
      let hwnd = key_to_hwnd(key);
      Self::apply_restore_metrics(hwnd, &managed.original);
      clog!("Restored: {}", managed.title);
    }

    self.config.clear_processes();
    let _ = self.config.save(&self.config_path);
    self.process_name_cache.recheck_monitored(&self.config);

    self.sync_tray();
  }

  fn toggle_window(&mut self, hwnd: HWND) {
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

    let Some(metrics) = Self::apply_borderless(hwnd, &process_name, None)
    else {
      return;
    };

    let title = get_window_title(hwnd);
    self.track_managed_window(key, process_name.clone(), title, metrics);

    if !self.config.is_monitored(&process_name) {
      self.config.add_process((*process_name).to_owned());
      let _ = self.config.save(&self.config_path);
      self.process_name_cache.recheck_monitored(&self.config);
    }
    self.sync_tray();
  }

  #[inline(always)]
  pub fn toggle_window_by_handle(&mut self, hwnd: HWND) {
    self.toggle_window(hwnd);
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
      tray::MENU_ID_SHOW_CONSOLE => {
        self.console_visible = !self.console_visible;
        set_console_visibility(self.console_visible);
        self.sync_tray();
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
