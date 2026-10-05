#![cfg(windows)]

use std::sync::atomic::Ordering;

use anyhow::Result;
use tray_icon::{
  menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
  Icon, TrayIcon, TrayIconBuilder,
};

use crate::{
  enforcer::VolumeState, registry::is_autorun_registered,
  utils::wake_main_thread,
};

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
const ICON_RADIUS: i32 = 6;

const fn generate_circle_icon(r: u8, g: u8, b: u8) -> [u8; BUFFER_LEN] {
  let mut rgba = [0u8; BUFFER_LEN];
  let mut y = 0;
  while y < HEIGHT {
    let mut x = 0;
    while x < WIDTH {
      // Pixels outside the circle stay at the zero-initialized
      // transparent value, so only the disc needs writing.
      let idx = ((y * WIDTH + x) * 4) as usize;
      let dx = x as i32 - WIDTH as i32 / 2;
      let dy = y as i32 - HEIGHT as i32 / 2;
      if dx * dx + dy * dy < ICON_RADIUS * ICON_RADIUS {
        rgba[idx] = r;
        rgba[idx + 1] = g;
        rgba[idx + 2] = b;
        rgba[idx + 3] = 255;
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

fn format_tooltip(state: &VolumeState) -> String {
  format!(
    "Input: {}% ({})\nOutput: {}% ({})",
    state.input_target.load(Ordering::SeqCst),
    status_str(state.input_paused.load(Ordering::SeqCst)),
    state.output_target.load(Ordering::SeqCst),
    status_str(state.output_paused.load(Ordering::SeqCst)),
  )
}

fn build_icon(input_paused: bool, output_paused: bool) -> Icon {
  let rgba = match (input_paused, output_paused) {
    (true, true) => GRAY_ICON_RGBA,
    (false, false) => RED_ICON_RGBA,
    _ => ORANGE_ICON_RGBA,
  };
  Icon::from_rgba(rgba.to_vec(), WIDTH, HEIGHT)
    .expect("built-in icon RGBA buffer is always valid")
}

impl TrayApp {
  pub fn new(state: &VolumeState, main_thread_id: u32) -> Result<Self> {
    let input_paused = state.input_paused.load(Ordering::SeqCst);
    let output_paused = state.output_paused.load(Ordering::SeqCst);

    let tray_menu = Menu::new();

    // 1. Input enforcement item
    let menu_item_toggle_input =
      MenuItem::new(toggle_text(input_paused, "input"), true, None);

    // 2. Output enforcement item
    let menu_item_toggle_output =
      MenuItem::new(toggle_text(output_paused, "output"), true, None);

    // 3. Set target volume
    let menu_item_set_target = MenuItem::new("Edit settings", true, None);

    // 4. Install/Remove autorun item
    let menu_item_autorun = MenuItem::new(autorun_text_label(), true, None);

    // 5. Exit item
    let menu_item_exit = MenuItem::new("Exit", true, None);

    let _ = tray_menu.append(&menu_item_toggle_input);
    let _ = tray_menu.append(&menu_item_toggle_output);
    let _ = tray_menu.append(&menu_item_set_target);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_autorun);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&menu_item_exit);

    let tray_icon = TrayIconBuilder::new()
      .with_menu(Box::new(tray_menu))
      .with_tooltip(format_tooltip(state))
      .with_icon(build_icon(input_paused, output_paused))
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

  pub fn refresh(&self, state: &VolumeState) {
    let input_paused = state.input_paused.load(Ordering::SeqCst);
    let output_paused = state.output_paused.load(Ordering::SeqCst);

    self
      .menu_item_toggle_input
      .set_text(toggle_text(input_paused, "input"));
    self
      .menu_item_toggle_output
      .set_text(toggle_text(output_paused, "output"));
    let _ = self
      .tray_icon
      .set_icon(Some(build_icon(input_paused, output_paused)));
    let _ = self.tray_icon.set_tooltip(Some(format_tooltip(state)));
  }

  pub fn refresh_autorun_menu(&self) {
    self.menu_item_autorun.set_text(autorun_text_label());
  }

  pub fn handle_events(&self) -> Option<TrayAction> {
    let id = MenuEvent::receiver().try_recv().ok()?.id;

    let action = if id == self.menu_item_toggle_input.id() {
      TrayAction::ToggleInput
    } else if id == self.menu_item_toggle_output.id() {
      TrayAction::ToggleOutput
    } else if id == self.menu_item_set_target.id() {
      TrayAction::PromptSetTarget
    } else if id == self.menu_item_autorun.id() {
      TrayAction::ToggleAutorun
    } else if id == self.menu_item_exit.id() {
      TrayAction::Exit
    } else {
      return None;
    };

    // Only a recognised action has a loop waiting on this thread.
    wake_main_thread(self.main_thread_id);
    Some(action)
  }
}
