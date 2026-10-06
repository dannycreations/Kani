mod logs;
mod queue;
mod settings;

use std::{
  sync::{mpsc, Arc, Mutex, OnceLock},
  thread,
};

use gpui_kit::{
  component::{
    h_flex,
    input::{InputEvent, InputState},
    tab::{Tab, TabBar},
    v_flex, ActiveTheme, Selectable,
  },
  div,
  prelude::*,
  App, Context, Entity, Focusable, IntoElement, MouseDownEvent, ParentElement,
  Render, Styled, Window,
};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::{
  core::queue::AppState,
  ffmpeg::{kill_all_children, AudioSettings},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
  Queue,
  Settings,
}

pub struct RenderApp {
  pub(super) state: Arc<Mutex<AppState>>,
  pub(super) selected_job_id: Option<usize>,
  pub(super) expanded_job_id: Option<usize>,
  pub(super) active_tab: AppTab,
  pub(super) item_inputs: Vec<(usize, Vec<Entity<InputState>>)>,
  pub(super) ffmpeg_path_state: Entity<InputState>,
  pub(super) parallel_jobs_state: Entity<InputState>,
}

static ACTIVE_STATE: OnceLock<Arc<Mutex<AppState>>> = OnceLock::new();

pub fn confirm_action(title: &str, description: &str) -> bool {
  let (tx, rx) = mpsc::channel();
  let title = title.to_string();
  let description = description.to_string();
  thread::spawn(move || {
    let result = MessageDialog::new()
      .set_title(title)
      .set_description(description)
      .set_buttons(MessageButtons::YesNo)
      .set_level(MessageLevel::Warning)
      .show();
    let _ = tx.send(matches!(result, MessageDialogResult::Yes));
  });
  rx.recv().unwrap_or(true)
}

pub fn confirm_quit() -> bool {
  let is_rendering = ACTIVE_STATE
    .get()
    .and_then(|state| state.lock().ok())
    .map(|state| state.is_running)
    .unwrap_or(false);

  if is_rendering {
    confirm_action(
      "Confirm Exit",
      "Rendering is in progress. Are you sure you want to quit?",
    )
  } else {
    true
  }
}

pub(super) fn blur_on_click_outside(
  input: Entity<InputState>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
  move |_, window, cx| {
    if input.read(cx).focus_handle(cx).is_focused(window) {
      input.update(cx, |input, cx| {
        input.unselect(window, cx);
      });
      window.blur(cx);
    }
  }
}

impl RenderApp {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let state = Arc::new(Mutex::new(AppState::new()));
    let _ = ACTIVE_STATE.set(Arc::clone(&state));

    let ffmpeg_path_state =
      cx.new(|cx| InputState::new(window, cx).default_value("ffmpeg"));

    cx.subscribe(&ffmpeg_path_state, move |this, entity, event, cx| {
      if let InputEvent::Change = event {
        let val = entity.read(cx).value();
        this.state.lock().unwrap().ffmpeg_path = Arc::from(val);
        cx.notify();
      }
    })
    .detach();

    let parallel_jobs_state =
      cx.new(|cx| InputState::new(window, cx).default_value("2"));

    cx.subscribe(&parallel_jobs_state, move |this, entity, event, cx| {
      if let InputEvent::Change = event {
        let val = entity.read(cx).value();
        if let Ok(jobs) = val.parse::<usize>() {
          if jobs > 0 {
            this.state.lock().unwrap().parallel_jobs = jobs;
          }
        }
        cx.notify();
      }
    })
    .detach();

    Self {
      state,
      selected_job_id: None,
      expanded_job_id: None,
      active_tab: AppTab::Queue,
      item_inputs: Vec::new(),
      ffmpeg_path_state,
      parallel_jobs_state,
    }
  }

  pub(super) fn get_inputs(
    &self,
    id: usize,
  ) -> Option<&Vec<Entity<InputState>>> {
    self
      .item_inputs
      .iter()
      .find(|(job_id, _)| *job_id == id)
      .map(|(_, inputs)| inputs)
  }

  pub(super) fn remove_inputs(&mut self, id: usize) {
    self.item_inputs.retain(|(job_id, _)| *job_id != id);
  }

  pub(super) fn rebuild_input_states(
    &mut self,
    id: usize,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.remove_inputs(id);
    let settings = {
      let state = self.state.lock().unwrap();
      state
        .queue
        .iter()
        .find(|item| item.id == id)
        .map(|item| item.settings.clone())
    };
    if let Some(settings) = settings {
      self.ensure_input_states(id, &settings, window, cx);
    }
  }

  pub(super) fn ensure_input_states(
    &mut self,
    id: usize,
    settings: &AudioSettings,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if self.get_inputs(id).is_some() {
      return;
    }

    let mut inputs = Vec::new();

    for (track_idx, track_config) in settings.tracks.iter().enumerate() {
      let offset = track_config.offset;
      let input_state = cx.new(|cx| {
        InputState::new(window, cx).default_value(format!("{:.0}", offset))
      });

      // Subscribe to input changes/blur
      cx.subscribe_in(
        &input_state,
        window,
        move |this, input_entity, event, _window, cx| match event {
          InputEvent::Change => {
            let val_str = input_entity.read(cx).value();
            if let Ok(val) = val_str.parse::<f32>() {
              let clamped_val = val.clamp(-30.0, 0.0);
              let mut state = this.state.lock().unwrap();
              if let Some(item) =
                state.queue.iter_mut().find(|item| item.id == id)
              {
                if let Some(tc) = item.settings.tracks.get_mut(track_idx) {
                  tc.offset = clamped_val;
                }
              }
              cx.notify();
            }
          }
          InputEvent::Blur | InputEvent::PressEnter { .. } => {
            let mut current_offset = offset;
            {
              let state = this.state.lock().unwrap();
              if let Some(item) = state.queue.iter().find(|item| item.id == id)
              {
                if let Some(tc) = item.settings.tracks.get(track_idx) {
                  current_offset = tc.offset;
                }
              }
            }
            if let Some(inputs) = this.get_inputs(id) {
              if let Some(input) = inputs.get(track_idx) {
                input.update(cx, |input, cx| {
                  input.set_value(
                    format!("{:.0}", current_offset),
                    _window,
                    cx,
                  );
                });
              }
            }
            cx.notify();
          }
          _ => {}
        },
      )
      .detach();

      inputs.push(input_state);
    }

    self.item_inputs.push((id, inputs));
  }
}

impl Drop for RenderApp {
  fn drop(&mut self) {
    kill_all_children();
  }
}

impl Render for RenderApp {
  fn render(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let state = self.state.lock().unwrap();
    let is_running = state.is_running;
    let view = cx.entity().downgrade();

    // Only the visible tab is built; the queue panel alone walks every item.
    let panel = match self.active_tab {
      AppTab::Queue => self.render_queue_panel(window, &state, &view, cx),
      AppTab::Settings => self
        .render_settings_panel(is_running, state.enable_parallel, &view)
        .into_any_element(),
    };

    let status_indicator = if is_running {
      div().text_color(cx.theme().info).child("Running")
    } else {
      div().text_color(cx.theme().muted_foreground).child("Idle")
    };

    v_flex()
      .p_4()
      .gap_4()
      .size_full()
      .child(
        h_flex()
          .w_full()
          .justify_between()
          .items_center()
          .child(
            TabBar::new("app_tabs")
              .underline()
              .child(
                Tab::new()
                  .label("Queue")
                  .selected(self.active_tab == AppTab::Queue)
                  .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                      if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| {
                          this.active_tab = AppTab::Queue;
                          cx.notify();
                        });
                      }
                    }
                  }),
              )
              .child(
                Tab::new()
                  .label("Settings")
                  .selected(self.active_tab == AppTab::Settings)
                  .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                      if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| {
                          this.active_tab = AppTab::Settings;
                          cx.notify();
                        });
                      }
                    }
                  }),
              ),
          )
          .child(status_indicator),
      )
      .child(panel)
  }
}
