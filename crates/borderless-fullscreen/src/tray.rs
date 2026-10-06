use rustc_hash::FxHashMap;
use tray_icon::{
  menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
  Icon,
};

use crate::app::ManagedWindow;

#[cfg(test)]
#[path = "tray_test.rs"]
mod tray_test;

pub const MENU_ID_RESTORE_ALL: &str = "restore_all";
pub const MENU_ID_PICK_WINDOW: &str = "pick_window";
pub const MENU_ID_SETTING_AUTOSTART: &str = "setting_autostart";
pub const MENU_ID_OPEN_CONFIG: &str = "open_config";
pub const MENU_ID_EXIT: &str = "exit";
pub const MENU_ID_RESTORE_WINDOW_PREFIX: &str = "restore_window:";

const TRAY_ICON_SIZE: u32 = 32;
const TRAY_ICON_LEN: usize = (TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4) as usize;

// Distance from the icon edge to the corner brackets, in pixels.
const BRACKET_INSET: i32 = 2;
// How far each bracket arm reaches along its edge.
const BRACKET_ARM: i32 = 7;
const BRACKET_COLOR: [u8; 4] = [0x00, 0xC0, 0xFF, 0xFF];
const PLATE_COLOR: [u8; 4] = [0x10, 0x18, 0x27, 0xDD];

const fn set_pixel(
  rgba: &mut [u8; TRAY_ICON_LEN],
  x: i32,
  y: i32,
  color: [u8; 4],
) {
  let idx = ((y as u32 * TRAY_ICON_SIZE + x as u32) * 4) as usize;
  rgba[idx] = color[0];
  rgba[idx + 1] = color[1];
  rgba[idx + 2] = color[2];
  rgba[idx + 3] = color[3];
}

const fn generate_tray_icon_rgba() -> [u8; TRAY_ICON_LEN] {
  let mut rgba = [0u8; TRAY_ICON_LEN];
  let size = TRAY_ICON_SIZE as i32;
  let near = BRACKET_INSET;
  let far = size - 1 - BRACKET_INSET;
  let plate_lo = near + 1;
  let plate_hi = size - near - 1;

  let mut y = plate_lo;
  while y < plate_hi {
    let mut x = plate_lo;
    while x < plate_hi {
      set_pixel(&mut rgba, x, y, PLATE_COLOR);
      x += 1;
    }
    y += 1;
  }

  // Arms run from a corner toward the plate center, so the corner on the
  // upper-left side steps `+1` and the one on the lower-right side steps `-1`.
  let mut corner_y = near;
  while corner_y <= far {
    let sy = if corner_y == near { 1 } else { -1 };
    let mut corner_x = near;
    while corner_x <= far {
      let sx = if corner_x == near { 1 } else { -1 };
      let mut arm = 0;
      while arm < BRACKET_ARM {
        let x = corner_x + sx * arm;
        let y = corner_y + sy * arm;
        set_pixel(&mut rgba, x, corner_y, BRACKET_COLOR);
        set_pixel(&mut rgba, x, corner_y + sy, BRACKET_COLOR);
        set_pixel(&mut rgba, corner_x, y, BRACKET_COLOR);
        set_pixel(&mut rgba, corner_x + sx, y, BRACKET_COLOR);
        arm += 1;
      }
      corner_x += far - near;
    }
    corner_y += far - near;
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

pub fn build_menu(
  managed: &FxHashMap<usize, ManagedWindow>,
  autostart_enabled: bool,
) -> Menu {
  let menu = Menu::new();

  let active_submenu =
    Submenu::new(format!("Active Windows ({})", managed.len()), true);

  if managed.is_empty() {
    let _ = active_submenu.append(&MenuItem::new("(none)", false, None));
  } else {
    for (&key, meta) in managed {
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
  let _ = menu.append(&PredefinedMenuItem::separator());

  let _ = menu.append(&MenuItem::with_id(MENU_ID_EXIT, "Exit", true, None));

  menu
}
