use tray_icon::{
  menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
  Icon,
};

use crate::app::App;

pub const MENU_ID_RESTORE_ALL: &str = "restore_all";
pub const MENU_ID_PICK_WINDOW: &str = "pick_window";
pub const MENU_ID_SETTING_AUTOSTART: &str = "setting_autostart";
pub const MENU_ID_OPEN_CONFIG: &str = "open_config";
pub const MENU_ID_SHOW_CONSOLE: &str = "show_console";
pub const MENU_ID_EXIT: &str = "exit";
pub const MENU_ID_RESTORE_WINDOW_PREFIX: &str = "restore_window:";

pub fn build_tray_icon() -> Icon {
  const SIZE: u32 = 32;
  const LEN: usize = (SIZE * SIZE * 4) as usize;
  let mut rgba = [0u8; LEN];

  let color_bracket = [0x00, 0xC0, 0xFF, 0xFF];
  let color_bg = [0x10, 0x18, 0x27, 0xDD];

  for y in 3..29 {
    for x in 3..29 {
      let idx = ((y * SIZE + x) * 4) as usize;
      rgba[idx..idx + 4].copy_from_slice(&color_bg);
    }
  }

  let set_pixel = |rgba: &mut [u8; LEN], x: u32, y: u32| {
    let idx = ((y * SIZE + x) * 4) as usize;
    rgba[idx..idx + 4].copy_from_slice(&color_bracket);
  };

  for i in 2..9 {
    set_pixel(&mut rgba, i, 2);
    set_pixel(&mut rgba, i, 3);
    set_pixel(&mut rgba, 2, i);
    set_pixel(&mut rgba, 3, i);

    set_pixel(&mut rgba, SIZE - 1 - i, 2);
    set_pixel(&mut rgba, SIZE - 1 - i, 3);
    set_pixel(&mut rgba, SIZE - 3, i);
    set_pixel(&mut rgba, SIZE - 4, i);

    set_pixel(&mut rgba, i, SIZE - 3);
    set_pixel(&mut rgba, i, SIZE - 4);
    set_pixel(&mut rgba, 2, SIZE - 1 - i);
    set_pixel(&mut rgba, 3, SIZE - 1 - i);

    set_pixel(&mut rgba, SIZE - 1 - i, SIZE - 3);
    set_pixel(&mut rgba, SIZE - 1 - i, SIZE - 4);
    set_pixel(&mut rgba, SIZE - 3, SIZE - 1 - i);
    set_pixel(&mut rgba, SIZE - 4, SIZE - 1 - i);
  }

  Icon::from_rgba(rgba.to_vec(), SIZE, SIZE).unwrap_or_else(|_| {
    Icon::from_rgba(vec![255, 255, 255, 255], 1, 1).unwrap()
  })
}

pub fn build_menu(app: &App, autostart_enabled: bool) -> Menu {
  let menu = Menu::new();
  let managed = app.managed_windows();

  let active_submenu =
    Submenu::new(format!("Active Windows ({})", managed.len()), true);
  if managed.is_empty() {
    let _ = active_submenu.append(&MenuItem::new("(none)", false, None));
  } else {
    for (key, meta) in managed {
      let label = format!("{} ({})", meta.title, meta.process_name);
      let id = format!("{MENU_ID_RESTORE_WINDOW_PREFIX}{key:x}");
      let _ = active_submenu.append(&MenuItem::with_id(id, label, true, None));
    }
    let _ = active_submenu.append(&PredefinedMenuItem::separator());
    let _ = active_submenu.append(&MenuItem::with_id(
      MENU_ID_RESTORE_ALL,
      "Restore All",
      true,
      None,
    ));
  }
  let _ = menu.append(&active_submenu);
  let _ = menu.append(&MenuItem::with_id(
    MENU_ID_PICK_WINDOW,
    "Pick Window...",
    true,
    None,
  ));
  let _ = menu.append(&PredefinedMenuItem::separator());

  let _ = menu.append(&CheckMenuItem::with_id(
    MENU_ID_SETTING_AUTOSTART,
    "Start with Windows",
    true,
    autostart_enabled,
    None,
  ));

  let _ = menu.append(&PredefinedMenuItem::separator());
  let _ = menu.append(&MenuItem::with_id(
    MENU_ID_OPEN_CONFIG,
    "Open Config File",
    true,
    None,
  ));
  let _ = menu.append(&CheckMenuItem::with_id(
    MENU_ID_SHOW_CONSOLE,
    "Show Console",
    true,
    app.console_visible(),
    None,
  ));
  let _ = menu.append(&PredefinedMenuItem::separator());

  let _ = menu.append(&MenuItem::with_id(MENU_ID_EXIT, "Exit", true, None));

  menu
}
