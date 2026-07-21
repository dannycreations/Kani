mod logs;
mod queue;
mod settings;

use std::{
  collections::HashSet,
  path::Path,
  sync::{mpsc, Arc, Mutex, OnceLock},
  thread,
  time::Duration,
};

use gpui::{
  div, prelude::*, px, AsyncApp, Context, Entity, FontWeight, IntoElement,
  ParentElement, Render, Styled, Timer, Window,
};
use gpui_component::{
  button::{Button, ButtonVariants as _},
  divider::Divider,
  h_flex,
  input::{InputEvent, InputState},
  scroll::ScrollableElement as _,
  tab::{Tab, TabBar},
  v_flex, ActiveTheme, Disableable, Selectable,
};
use rfd::{
  FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel,
};

use crate::{
  ffmpeg::{kill_all_children, AudioSettings},
  queue::{AppState, QueueItemStatus},
  IconName,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
  Queue,
  Settings,
}

pub struct TrackInputState {
  pub input_state: Entity<InputState>,
}

pub struct ItemInputStates {
  pub tracks: Vec<TrackInputState>,
}

pub struct RenderApp {
  pub(super) state: Arc<Mutex<AppState>>,
  pub(super) selected_job_id: Option<usize>,
  pub(super) expanded_job_id: Option<usize>,
  pub(super) active_tab: AppTab,

  // Per-video config input states mapped by job ID
  pub(super) item_inputs: Vec<(usize, ItemInputStates)>,

  // Global config input state
  pub(super) ffmpeg_path_state: Entity<InputState>,
  pub(super) parallel_jobs_state: Entity<InputState>,
}

pub static ACTIVE_STATE: OnceLock<Arc<Mutex<AppState>>> = OnceLock::new();

pub fn set_active_app_state(state: Arc<Mutex<AppState>>) {
  let _ = ACTIVE_STATE.set(state);
}

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

impl RenderApp {
  pub fn state(&self) -> &Arc<Mutex<AppState>> {
    &self.state
  }

  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let state = Arc::new(Mutex::new(AppState::new()));

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

  pub(super) fn get_inputs(&self, id: usize) -> Option<&ItemInputStates> {
    self
      .item_inputs
      .iter()
      .find(|(k, _)| *k == id)
      .map(|(_, v)| v)
  }

  pub(super) fn has_inputs(&self, id: usize) -> bool {
    self.item_inputs.iter().any(|(k, _)| *k == id)
  }

  pub(super) fn remove_inputs(&mut self, id: usize) {
    self.item_inputs.retain(|(k, _)| *k != id);
  }

  pub(super) fn ensure_input_states(
    &mut self,
    id: usize,
    settings: &AudioSettings,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if self.has_inputs(id) {
      return;
    }

    let mut tracks_vec = Vec::new();

    for (track_idx, track_config) in settings.tracks.iter().enumerate() {
      let val = track_config.offset;
      let input_state = cx.new(|cx| {
        InputState::new(window, cx).default_value(format!("{:.0}", val))
      });

      // Capture default offset for the blur/reset fallback
      let default_offset = val;

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
            let mut current_offset = default_offset;
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
              if let Some(track_input) = inputs.tracks.get(track_idx) {
                track_input.input_state.update(cx, |input, cx| {
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

      tracks_vec.push(TrackInputState { input_state });
    }

    self
      .item_inputs
      .push((id, ItemInputStates { tracks: tracks_vec }));
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
    let _selected_id = self.selected_job_id;

    let width = window.viewport_size().width;
    let use_two_columns = width > px(600.0);

    let view = cx.entity().downgrade();

    let enable_parallel = state.enable_parallel;
    let settings_panel =
      self.render_settings_panel(is_running, enable_parallel, &view, cx);

    let start_stop_btn = if is_running {
      Button::new("stop")
        .danger()
        .icon(IconName::Stop)
        .compact()
        .tooltip("Stop")
        .on_click({
          let view = view.clone();
          move |_, _, cx| {
            if confirm_action(
              "Confirm Stop",
              "Rendering is currently in progress. Are you sure you want to stop?",
            ) {
              if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| {
                  this.state.lock().unwrap().stop();
                  cx.notify();
                });
              }
            }
          }
        })
    } else {
      let has_pending = state
        .queue
        .iter()
        .any(|item| matches!(item.status, QueueItemStatus::Pending));
      Button::new("start")
        .success()
        .icon(IconName::Play)
        .compact()
        .tooltip("Start")
        .disabled(!has_pending)
        .on_click({
          let view = view.clone();
          move |_, _, cx| {
            let view_weak = view.clone();
            if let Some(view) = view.upgrade() {
              view.update(cx, |this, cx| {
                let state_clone = Arc::clone(&this.state);
                this.state.lock().unwrap().start(state_clone);

                // Spawn a timer loop to refresh the window while running
                let state_clone = Arc::clone(&this.state);
                cx.spawn(|_, cx: &mut AsyncApp| {
                  let cx = cx.clone();
                  async move {
                    loop {
                      let (is_running, active_id) = {
                        let state = state_clone.lock().unwrap();
                        let active_id =
                          state.active_processes.last().map(|(id, _)| *id);
                        (state.is_running, active_id)
                      };

                      let _ = cx.update(|cx| {
                        if let Some(view) = view_weak.upgrade() {
                          view.update(cx, |this, cx| {
                            if let Some(id) = active_id {
                              if this.selected_job_id != Some(id) {
                                this.selected_job_id = Some(id);
                                cx.notify();
                              }
                            }
                          });
                        }
                        cx.refresh_windows();
                      });

                      if !is_running {
                        break;
                      }
                      Timer::after(Duration::from_millis(100)).await;
                    }
                  }
                })
                .detach();

                cx.notify();
              });
            }
          }
        })
    };

    let add_files_btn = Button::new("add_files")
      .icon(IconName::Plus)
      .compact()
      .tooltip("Add Video Files")
      .on_click({
        let view = view.clone();
        move |_, _, cx| {
          let view = view.clone();
          cx.spawn(|cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
              let files = FileDialog::new()
                .add_filter(
                  "Video Files",
                  &["mkv", "mp4", "avi", "mov", "webm", "flv"],
                )
                .pick_files();
              if let Some(files) = files {
                let _ = cx.update(|cx| {
                  if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                      let mut state = this.state.lock().unwrap();
                      for file in files {
                        state.add_file(file.to_string_lossy().to_string());
                      }
                      cx.notify();
                    });
                  }
                });
              }
            }
          })
          .detach();
        }
      });

    let has_completed = state
      .queue
      .iter()
      .any(|item| matches!(item.status, QueueItemStatus::Completed { .. }));
    let has_items = !state.queue.is_empty();

    let clear_completed_btn = Button::new("clear_completed")
      .warning()
      .icon(IconName::Check)
      .compact()
      .tooltip("Clear Completed")
      .disabled(!has_completed)
      .on_click({
        let view = view.clone();
        move |_, _, cx| {
          if let Some(view) = view.upgrade() {
            view.update(cx, |this, cx| {
              this.state.lock().unwrap().clear_completed();
              let queue_ids: HashSet<usize> = this
                .state
                .lock()
                .unwrap()
                .queue
                .iter()
                .map(|item| item.id)
                .collect();
              this.item_inputs.retain(|(id, _)| queue_ids.contains(id));
              if let Some(expanded_id) = this.expanded_job_id {
                if !queue_ids.contains(&expanded_id) {
                  this.expanded_job_id = None;
                }
              }
              if let Some(selected_id) = this.selected_job_id {
                if !queue_ids.contains(&selected_id) {
                  this.selected_job_id = None;
                }
              }
              cx.notify();
            });
          }
        }
      });

    let clear_all_btn = Button::new("clear_all")
      .danger()
      .icon(IconName::Delete)
      .compact()
      .tooltip("Clear All")
      .disabled(!has_items)
      .on_click({
        let view = view.clone();
        move |_, _, cx| {
          if let Some(view) = view.upgrade() {
            view.update(cx, |this, cx| {
              this.state.lock().unwrap().clear_all();
              this.item_inputs.clear();
              this.expanded_job_id = None;
              this.selected_job_id = None;
              cx.notify();
            });
          }
        }
      });

    let mut queue_items_elements = Vec::new();
    if state.queue.is_empty() {
      queue_items_elements.push(
        div()
          .text_color(cx.theme().muted_foreground)
          .child("Queue is empty. Add video files to begin.")
          .into_any_element(),
      );
    } else {
      for (item_idx, item) in state.queue.iter().enumerate() {
        let is_selected = self.selected_job_id == Some(item.id);
        let is_expanded = self.expanded_job_id == Some(item.id);

        let filename = Path::new(&*item.input_path)
          .file_name()
          .and_then(|f| f.to_str())
          .unwrap_or(&item.input_path)
          .to_string();
        let display_name = format!("{}. {}", item_idx + 1, filename);

        let queue_item_el = self.render_queue_item(
          item,
          item_idx,
          is_selected,
          is_expanded,
          is_running,
          display_name,
          &view,
          window,
          cx,
        );

        queue_items_elements.push(queue_item_el.into_any_element());
      }
    }

    let left_col = v_flex()
      .gap_2()
      .h_full()
      .child(
        div()
          .font_weight(FontWeight::BOLD)
          .text_lg()
          .child("Job Queue"),
      )
      .child(
        h_flex()
          .gap_2()
          .child(add_files_btn)
          .child(start_stop_btn)
          .child(clear_completed_btn)
          .child(clear_all_btn),
      )
      .child(Divider::horizontal())
      .child(
        v_flex()
          .flex_1()
          .overflow_y_scrollbar()
          .gap_2()
          .children(queue_items_elements),
      );

    let right_col = self.render_logs_panel(self.selected_job_id, &state, cx);

    let status_indicator = if is_running {
      div().text_color(cx.theme().info).child("Running")
    } else {
      div().text_color(cx.theme().muted_foreground).child("Idle")
    };

    let app_tab_bar = h_flex()
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
      .child(status_indicator);

    let queue_panel = if use_two_columns {
      div()
        .flex()
        .flex_row()
        .w_full()
        .flex_grow()
        .h_full()
        .gap_4()
        .child(div().flex().flex_col().flex_1().h_full().child(left_col))
        .child(div().flex().flex_col().flex_1().h_full().child(right_col))
    } else {
      div()
        .flex()
        .flex_col()
        .w_full()
        .flex_grow()
        .h_full()
        .gap_4()
        .child(
          div()
            .flex()
            .flex_col()
            .w_full()
            .h(px(250.0))
            .child(left_col),
        )
        .child(
          div()
            .flex()
            .flex_col()
            .w_full()
            .flex_grow()
            .h_full()
            .child(right_col),
        )
    };

    v_flex().p_4().gap_4().size_full().child(app_tab_bar).child(
      match self.active_tab {
        AppTab::Queue => queue_panel.into_any_element(),
        AppTab::Settings => settings_panel.into_any_element(),
      },
    )
  }
}
