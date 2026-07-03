#![cfg(windows)]

use anyhow::Result;
use tray_icon::{
  menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
  Icon, TrayIcon, TrayIconBuilder,
};

use crate::registry::is_autorun_registered;

pub enum TrayAction {
  ToggleEnforcement,
  PromptSetTarget,
  ToggleAutorun,
  Exit,
}

pub struct TrayApp {
  tray_icon: TrayIcon,
  menu_item_toggle: MenuItem,
  menu_item_set_target: MenuItem,
  menu_item_autorun: MenuItem,
  menu_item_exit: MenuItem,
}

const WIDTH: u32 = 16;
const HEIGHT: u32 = 16;
const BUFFER_LEN: usize = (WIDTH * HEIGHT * 4) as usize;

const fn generate_circle_icon(r: u8, g: u8, b: u8) -> [u8; BUFFER_LEN] {
  let mut rgba = [0u8; BUFFER_LEN];
  let mut y = 0;
  while y < HEIGHT {
    let mut x = 0;
    while x < WIDTH {
      let idx = ((y * WIDTH + x) * 4) as usize;
      let dx = x as i32 - 8;
      let dy = y as i32 - 8;
      if dx * dx + dy * dy < 36 {
        rgba[idx] = r;
        rgba[idx + 1] = g;
        rgba[idx + 2] = b;
        rgba[idx + 3] = 255;
      } else {
        rgba[idx] = 0;
        rgba[idx + 1] = 0;
        rgba[idx + 2] = 0;
        rgba[idx + 3] = 0;
      }
      x += 1;
    }
    y += 1;
  }
  rgba
}

const RED_ICON_RGBA: [u8; BUFFER_LEN] = generate_circle_icon(220, 20, 60);
const GRAY_ICON_RGBA: [u8; BUFFER_LEN] = generate_circle_icon(128, 128, 128);

impl TrayApp {
  pub fn new(target_percent: u32, is_paused: bool) -> Result<Self> {
    let rgba = if is_paused {
      &GRAY_ICON_RGBA
    } else {
      &RED_ICON_RGBA
    };
    let icon = Icon::from_rgba(rgba.to_vec(), WIDTH, HEIGHT)?;

    let tray_menu = Menu::new();

    // 1. Pause/Resume enforcement item
    let toggle_text = if is_paused {
      "Resume enforcement"
    } else {
      "Pause enforcement"
    };
    let menu_item_toggle = MenuItem::new(toggle_text, true, None);

    // 2. Set target volume
    let menu_item_set_target = MenuItem::new("Set target volume", true, None);

    // 3. Install/Remove autorun item
    let is_installed = is_autorun_registered();
    let autorun_text = if is_installed {
      "Remove autorun"
    } else {
      "Install autorun"
    };
    let menu_item_autorun = MenuItem::new(autorun_text, true, None);

    // 4. Exit item
    let menu_item_exit = MenuItem::new("Exit", true, None);

    let _ = tray_menu.append(&menu_item_toggle);
    let _ = tray_menu.append(&menu_item_set_target);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_autorun);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_exit);

    let tray_icon = TrayIconBuilder::new()
      .with_menu(Box::new(tray_menu))
      .with_tooltip(format!("VolLevelLock: {}%", target_percent))
      .with_icon(icon)
      .build()?;

    Ok(Self {
      tray_icon,
      menu_item_toggle,
      menu_item_set_target,
      menu_item_autorun,
      menu_item_exit,
    })
  }

  pub fn update_tooltip(&self, target_percent: u32) {
    let _ = self
      .tray_icon
      .set_tooltip(Some(format!("VolLevelLock: {}%", target_percent)));
  }

  pub fn update_toggle_text(&self, is_paused: bool) {
    let text = if is_paused {
      "Resume enforcement"
    } else {
      "Pause enforcement"
    };
    self.menu_item_toggle.set_text(text);
  }

  pub fn update_icon(&self, is_paused: bool) -> Result<()> {
    let rgba = if is_paused {
      &GRAY_ICON_RGBA
    } else {
      &RED_ICON_RGBA
    };
    let icon = Icon::from_rgba(rgba.to_vec(), WIDTH, HEIGHT)?;
    self.tray_icon.set_icon(Some(icon))?;
    Ok(())
  }

  pub fn refresh_autorun_menu(&self) {
    let is_installed = is_autorun_registered();
    let text = if is_installed {
      "Remove autorun"
    } else {
      "Install autorun"
    };
    self.menu_item_autorun.set_text(text);
  }

  pub fn handle_events(&self) -> Option<TrayAction> {
    if let Ok(event) = MenuEvent::receiver().try_recv() {
      if event.id == self.menu_item_toggle.id() {
        return Some(TrayAction::ToggleEnforcement);
      } else if event.id == self.menu_item_set_target.id() {
        return Some(TrayAction::PromptSetTarget);
      } else if event.id == self.menu_item_autorun.id() {
        return Some(TrayAction::ToggleAutorun);
      } else if event.id == self.menu_item_exit.id() {
        return Some(TrayAction::Exit);
      }
    }
    None
  }
}
