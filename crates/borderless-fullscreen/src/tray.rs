use std::fmt::Write as _;

use tray_icon::{
  menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
  Icon,
};

use crate::app::App;

pub const MENU_ID_RESTORE_ALL: &str = "restore_all";
pub const MENU_ID_PICK_WINDOW: &str = "pick_window";
pub const MENU_ID_SETTING_AUTOSTART: &str = "setting_autostart";
pub const MENU_ID_OPEN_CONFIG: &str = "open_config";
pub const MENU_ID_EXIT: &str = "exit";
pub const MENU_ID_RESTORE_WINDOW_PREFIX: &str = "restore_window:";

const TRAY_ICON_SIZE: u32 = 32;
const TRAY_ICON_LEN: usize = (TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4) as usize;

const fn generate_tray_icon_rgba() -> [u8; TRAY_ICON_LEN] {
  let mut rgba = [0u8; TRAY_ICON_LEN];
  let color_bracket = [0x00, 0xC0, 0xFF, 0xFF];
  let color_bg = [0x10, 0x18, 0x27, 0xDD];

  let mut y = 3;
  while y < 29 {
    let mut x = 3;
    while x < 29 {
      let idx = ((y * TRAY_ICON_SIZE + x) * 4) as usize;
      rgba[idx] = color_bg[0];
      rgba[idx + 1] = color_bg[1];
      rgba[idx + 2] = color_bg[2];
      rgba[idx + 3] = color_bg[3];
      x += 1;
    }
    y += 1;
  }

  macro_rules! set_pix {
    ($rgba:ident, $x:expr, $y:expr, $c:ident) => {
      let idx = (($y * TRAY_ICON_SIZE + $x) * 4) as usize;
      $rgba[idx] = $c[0];
      $rgba[idx + 1] = $c[1];
      $rgba[idx + 2] = $c[2];
      $rgba[idx + 3] = $c[3];
    };
  }

  let mut i = 2;
  while i < 9 {
    set_pix!(rgba, i, 2, color_bracket);
    set_pix!(rgba, i, 3, color_bracket);
    set_pix!(rgba, 2, i, color_bracket);
    set_pix!(rgba, 3, i, color_bracket);

    set_pix!(rgba, TRAY_ICON_SIZE - 1 - i, 2, color_bracket);
    set_pix!(rgba, TRAY_ICON_SIZE - 1 - i, 3, color_bracket);
    set_pix!(rgba, TRAY_ICON_SIZE - 3, i, color_bracket);
    set_pix!(rgba, TRAY_ICON_SIZE - 4, i, color_bracket);

    set_pix!(rgba, i, TRAY_ICON_SIZE - 3, color_bracket);
    set_pix!(rgba, i, TRAY_ICON_SIZE - 4, color_bracket);
    set_pix!(rgba, 2, TRAY_ICON_SIZE - 1 - i, color_bracket);
    set_pix!(rgba, 3, TRAY_ICON_SIZE - 1 - i, color_bracket);

    set_pix!(
      rgba,
      TRAY_ICON_SIZE - 1 - i,
      TRAY_ICON_SIZE - 3,
      color_bracket
    );
    set_pix!(
      rgba,
      TRAY_ICON_SIZE - 1 - i,
      TRAY_ICON_SIZE - 4,
      color_bracket
    );
    set_pix!(
      rgba,
      TRAY_ICON_SIZE - 3,
      TRAY_ICON_SIZE - 1 - i,
      color_bracket
    );
    set_pix!(
      rgba,
      TRAY_ICON_SIZE - 4,
      TRAY_ICON_SIZE - 1 - i,
      color_bracket
    );
    i += 1;
  }

  rgba
}

static TRAY_ICON_BYTES: [u8; TRAY_ICON_LEN] = generate_tray_icon_rgba();

pub fn build_tray_icon() -> Icon {
  Icon::from_rgba(TRAY_ICON_BYTES.to_vec(), TRAY_ICON_SIZE, TRAY_ICON_SIZE)
    .unwrap_or_else(|_| {
      Icon::from_rgba(vec![255, 255, 255, 255], 1, 1).unwrap()
    })
}

pub fn build_menu(app: &App, autostart_enabled: bool) -> Menu {
  let menu = Menu::new();
  let managed = app.managed_windows();

  let mut active_label = String::with_capacity(32);
  let _ = write!(active_label, "Active Windows ({})", managed.len());
  let active_submenu = Submenu::new(active_label, true);

  if managed.is_empty() {
    let _ = active_submenu.append(&MenuItem::new("(none)", false, None));
  } else {
    for (&key, meta) in managed {
      let mut label =
        String::with_capacity(meta.title.len() + meta.process_name.len() + 3);
      let _ = write!(label, "{} ({})", meta.title, meta.process_name);

      let mut id =
        String::with_capacity(MENU_ID_RESTORE_WINDOW_PREFIX.len() + 16);
      let _ = write!(id, "{MENU_ID_RESTORE_WINDOW_PREFIX}{key:x}");

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
  let _ = menu.append(&PredefinedMenuItem::separator());

  let _ = menu.append(&MenuItem::with_id(MENU_ID_EXIT, "Exit", true, None));

  menu
}
