#![cfg(windows)]

use anyhow::Result;
use tray_icon::{
  menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
  Icon, TrayIcon, TrayIconBuilder,
};

use crate::registry::is_autorun_registered;

pub enum TrayAction {
  ToggleInput,
  ToggleOutput,
  PromptSetTarget,
  ToggleAutorun,
  Exit,
}

pub struct TrayApp {
  tray_icon: TrayIcon,
  menu_item_toggle_input: MenuItem,
  menu_item_toggle_output: MenuItem,
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
const ORANGE_ICON_RGBA: [u8; BUFFER_LEN] = generate_circle_icon(255, 165, 0);
const GRAY_ICON_RGBA: [u8; BUFFER_LEN] = generate_circle_icon(128, 128, 128);

impl TrayApp {
  pub fn new(
    input_target: u32,
    input_paused: bool,
    output_target: u32,
    output_paused: bool,
  ) -> Result<Self> {
    let rgba = match (input_paused, output_paused) {
      (true, true) => &GRAY_ICON_RGBA,
      (false, false) => &RED_ICON_RGBA,
      _ => &ORANGE_ICON_RGBA,
    };
    let icon = Icon::from_rgba(rgba.to_vec(), WIDTH, HEIGHT)?;

    let tray_menu = Menu::new();

    // 1. Input enforcement item
    let toggle_input_text = if input_paused {
      "Resume input enforcement"
    } else {
      "Pause input enforcement"
    };
    let menu_item_toggle_input = MenuItem::new(toggle_input_text, true, None);

    // 2. Output enforcement item
    let toggle_output_text = if output_paused {
      "Resume output enforcement"
    } else {
      "Pause output enforcement"
    };
    let menu_item_toggle_output = MenuItem::new(toggle_output_text, true, None);

    // 3. Set target volume
    let menu_item_set_target = MenuItem::new("Edit settings", true, None);

    // 4. Install/Remove autorun item
    let is_installed = is_autorun_registered();
    let autorun_text = if is_installed {
      "Remove autorun"
    } else {
      "Install autorun"
    };
    let menu_item_autorun = MenuItem::new(autorun_text, true, None);

    // 5. Exit item
    let menu_item_exit = MenuItem::new("Exit", true, None);

    let _ = tray_menu.append(&menu_item_toggle_input);
    let _ = tray_menu.append(&menu_item_toggle_output);
    let _ = tray_menu.append(&menu_item_set_target);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_autorun);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_exit);

    let input_status = if input_paused { "Paused" } else { "Active" };
    let output_status = if output_paused { "Paused" } else { "Active" };
    let tooltip = format!(
      "Input: {}% ({})\nOutput: {}% ({})",
      input_target, input_status, output_target, output_status
    );

    let tray_icon = TrayIconBuilder::new()
      .with_menu(Box::new(tray_menu))
      .with_tooltip(tooltip)
      .with_icon(icon)
      .build()?;

    Ok(Self {
      tray_icon,
      menu_item_toggle_input,
      menu_item_toggle_output,
      menu_item_set_target,
      menu_item_autorun,
      menu_item_exit,
    })
  }

  pub fn update_tooltip(
    &self,
    input_target: u32,
    input_paused: bool,
    output_target: u32,
    output_paused: bool,
  ) {
    let input_status = if input_paused { "Paused" } else { "Active" };
    let output_status = if output_paused { "Paused" } else { "Active" };
    let tooltip = format!(
      "Input: {}% ({})\nOutput: {}% ({})",
      input_target, input_status, output_target, output_status
    );
    let _ = self.tray_icon.set_tooltip(Some(tooltip));
  }

  pub fn update_toggle_input_text(&self, is_paused: bool) {
    let text = if is_paused {
      "Resume input enforcement"
    } else {
      "Pause input enforcement"
    };
    self.menu_item_toggle_input.set_text(text);
  }

  pub fn update_toggle_output_text(&self, is_paused: bool) {
    let text = if is_paused {
      "Resume output enforcement"
    } else {
      "Pause output enforcement"
    };
    self.menu_item_toggle_output.set_text(text);
  }

  pub fn update_icon(
    &self,
    input_paused: bool,
    output_paused: bool,
  ) -> Result<()> {
    let rgba = match (input_paused, output_paused) {
      (true, true) => &GRAY_ICON_RGBA,
      (false, false) => &RED_ICON_RGBA,
      _ => &ORANGE_ICON_RGBA,
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
      if event.id == self.menu_item_toggle_input.id() {
        return Some(TrayAction::ToggleInput);
      } else if event.id == self.menu_item_toggle_output.id() {
        return Some(TrayAction::ToggleOutput);
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
