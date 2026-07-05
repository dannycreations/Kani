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
  main_thread_id: u32,
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
    main_thread_id: u32,
  ) -> Result<Self> {
    let icon = Self::get_icon_for_state(input_paused, output_paused)?;

    let tray_menu = Menu::new();

    // 1. Input enforcement item
    let menu_item_toggle_input =
      MenuItem::new(Self::toggle_text(input_paused, "input"), true, None);

    // 2. Output enforcement item
    let menu_item_toggle_output =
      MenuItem::new(Self::toggle_text(output_paused, "output"), true, None);

    // 3. Set target volume
    let menu_item_set_target = MenuItem::new("Edit settings", true, None);

    // 4. Install/Remove autorun item
    let menu_item_autorun =
      MenuItem::new(Self::autorun_text_label(), true, None);

    // 5. Exit item
    let menu_item_exit = MenuItem::new("Exit", true, None);

    let _ = tray_menu.append(&menu_item_toggle_input);
    let _ = tray_menu.append(&menu_item_toggle_output);
    let _ = tray_menu.append(&menu_item_set_target);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_autorun);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_exit);

    let tooltip = Self::format_tooltip(
      input_target,
      input_paused,
      output_target,
      output_paused,
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
      main_thread_id,
    })
  }

  fn status_str(is_paused: bool) -> &'static str {
    if is_paused {
      "Paused"
    } else {
      "Active"
    }
  }

  fn toggle_text(is_paused: bool, flow_name: &str) -> String {
    if is_paused {
      format!("Resume {} enforcement", flow_name)
    } else {
      format!("Pause {} enforcement", flow_name)
    }
  }

  fn autorun_text_label() -> &'static str {
    if is_autorun_registered() {
      "Remove autorun"
    } else {
      "Install autorun"
    }
  }

  fn format_tooltip(
    input_target: u32,
    input_paused: bool,
    output_target: u32,
    output_paused: bool,
  ) -> String {
    format!(
      "Input: {}% ({})\nOutput: {}% ({})",
      input_target,
      Self::status_str(input_paused),
      output_target,
      Self::status_str(output_paused)
    )
  }

  fn get_icon_for_state(
    input_paused: bool,
    output_paused: bool,
  ) -> Result<Icon> {
    let rgba = match (input_paused, output_paused) {
      (true, true) => &GRAY_ICON_RGBA,
      (false, false) => &RED_ICON_RGBA,
      _ => &ORANGE_ICON_RGBA,
    };
    Icon::from_rgba(rgba.to_vec(), WIDTH, HEIGHT).map_err(Into::into)
  }

  pub fn update_tooltip(
    &self,
    input_target: u32,
    input_paused: bool,
    output_target: u32,
    output_paused: bool,
  ) {
    let tooltip = Self::format_tooltip(
      input_target,
      input_paused,
      output_target,
      output_paused,
    );
    let _ = self.tray_icon.set_tooltip(Some(tooltip));
  }

  pub fn update_toggle_input_text(&self, is_paused: bool) {
    self
      .menu_item_toggle_input
      .set_text(Self::toggle_text(is_paused, "input"));
  }

  pub fn update_toggle_output_text(&self, is_paused: bool) {
    self
      .menu_item_toggle_output
      .set_text(Self::toggle_text(is_paused, "output"));
  }

  pub fn update_icon(
    &self,
    input_paused: bool,
    output_paused: bool,
  ) -> Result<()> {
    let icon = Self::get_icon_for_state(input_paused, output_paused)?;
    self.tray_icon.set_icon(Some(icon))?;
    Ok(())
  }

  pub fn refresh_autorun_menu(&self) {
    self.menu_item_autorun.set_text(Self::autorun_text_label());
  }

  pub fn handle_events(&self) -> Option<TrayAction> {
    if let Ok(event) = MenuEvent::receiver().try_recv() {
      unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
          self.main_thread_id,
          crate::WM_WAKEUP,
          windows::Win32::Foundation::WPARAM(0),
          windows::Win32::Foundation::LPARAM(0),
        );
      }
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
