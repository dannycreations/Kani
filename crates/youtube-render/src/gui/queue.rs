use std::{collections::HashSet, fs, path::Path, sync::Arc, time::Duration};

use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    progress::Progress,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme, Disableable,
  },
  div,
  prelude::*,
  px, AnyElement, AsyncApp, Context, Entity, FontWeight, IntoElement,
  ParentElement, SharedString, Styled, WeakEntity, Window,
};
use rfd::FileDialog;

use crate::{
  core::{
    assets::IconName,
    queue::{AppState, QueueItem, QueueItemStatus},
  },
  ffmpeg::{AudioSettings, Preset},
  gui::{blur_on_click_outside, confirm_action, RenderApp},
};

impl RenderApp {
  pub(super) fn render_queue_panel(
    &self,
    window: &Window,
    state: &AppState,
    view: &WeakEntity<Self>,
    cx: &mut Context<Self>,
  ) -> AnyElement {
    let is_running = state.is_running;
    let use_two_columns = window.viewport_size().width > px(600.0);

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
      let has_pending = state.queue.iter().any(|item| item.status.is_pending());
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
                AppState::start(&this.state);

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

                      cx.update(|cx| {
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
                      cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
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
                cx.update(|cx| {
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
              let queue_ids: HashSet<usize> = {
                let mut state = this.state.lock().unwrap();
                state.clear_completed();
                state.queue.iter().map(|item| item.id).collect()
              };
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
      .disabled(state.queue.is_empty())
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
      let queue_len = state.queue.len();
      for (item_idx, item) in state.queue.iter().enumerate() {
        queue_items_elements.push(
          self
            .render_queue_item(item, item_idx, queue_len, is_running, view, cx)
            .into_any_element(),
        );
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
      .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
      .child(
        v_flex()
          .flex_1()
          .overflow_y_scrollbar()
          .gap_2()
          .children(queue_items_elements),
      );

    let right_col = self.render_logs_panel(self.selected_job_id, state, cx);

    if use_two_columns {
      div()
        .flex()
        .flex_row()
        .w_full()
        .flex_grow(1.0)
        .h_full()
        .gap_4()
        .child(div().flex().flex_col().flex_1().h_full().child(left_col))
        .child(div().flex().flex_col().flex_1().h_full().child(right_col))
        .into_any_element()
    } else {
      div()
        .flex()
        .flex_col()
        .w_full()
        .flex_grow(1.0)
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
            .flex_grow(1.0)
            .h_full()
            .child(right_col),
        )
        .into_any_element()
    }
  }

  fn swap_tracks(
    &mut self,
    id: usize,
    track_idx: usize,
    other_idx: usize,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    {
      let mut state = self.state.lock().unwrap();
      if let Some(item) = state.queue.iter_mut().find(|item| item.id == id) {
        if other_idx < item.settings.tracks.len() {
          item.settings.tracks.swap(track_idx, other_idx);
        }
      }
    }
    self.rebuild_input_states(id, window, cx);
    cx.notify();
  }

  pub(super) fn render_queue_item(
    &self,
    item: &QueueItem,
    item_idx: usize,
    queue_len: usize,
    is_running: bool,
    view: &WeakEntity<Self>,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let id = item.id;
    let is_selected = self.selected_job_id == Some(id);
    let is_expanded = self.expanded_job_id == Some(id);
    let is_first = item_idx == 0;
    let is_last = item_idx + 1 == queue_len;

    let filename = Path::new(&*item.input_path)
      .file_name()
      .and_then(|f| f.to_str())
      .unwrap_or(&item.input_path);
    let display_name = format!("{}. {}", item_idx + 1, filename);

    let item_view = view.clone();
    let item_view_click = view.clone();

    let item_settings_panel = self.render_queue_item_settings(
      item,
      is_expanded,
      is_running,
      &item_view,
      cx,
    );

    let progress_panel = self.render_queue_item_progress(item, cx);

    v_flex()
      .p_2()
      .rounded_md()
      .border_1()
      .border_color(if is_selected {
        cx.theme().accent
      } else {
        cx.theme().border
      })
      .bg(if is_selected {
        cx.theme().accent.opacity(0.1)
      } else {
        cx.theme().transparent
      })
      .child(
        h_flex()
          .justify_between()
          .items_center()
          .child(
            div()
              .id(("filename", id))
              .cursor_pointer()
              .flex_1()
              .text_sm()
              .child(display_name)
              .on_click(move |_, _, cx| {
                if let Some(view) = item_view_click.upgrade() {
                  view.update(cx, |this, cx| {
                    this.selected_job_id = Some(id);
                    cx.notify();
                  });
                }
              }),
          )
          .child(self.render_queue_item_header_actions(
            item,
            is_expanded,
            is_first,
            is_last,
            &item_view,
            cx,
          )),
      )
      .when_some(item_settings_panel, |this, panel| this.child(panel))
      .when_some(progress_panel, |this, panel| this.child(panel))
  }

  fn render_preset_row(
    &self,
    id: usize,
    current_preset_idx: usize,
    item_settings: &AudioSettings,
    controls_disabled: bool,
    item_view: &WeakEntity<Self>,
  ) -> impl IntoElement {
    let mut presets = h_flex()
      .gap_2()
      .items_center()
      .child(div().text_xs().child("Preset:"));

    for (preset_idx, preset) in Preset::builtins().iter().enumerate() {
      let is_active_preset = current_preset_idx == preset_idx;
      let preset_name = preset.name.to_string();
      let item_view_preset = item_view.clone();
      presets = presets.child(
        Button::new(SharedString::from(format!(
          "preset_{}_{}",
          id, preset_idx
        )))
        .label(preset_name)
        .compact()
        .when(is_active_preset, |b| b.primary())
        .disabled(controls_disabled)
        .on_click(move |_, window, cx| {
          if let Some(view) = item_view_preset.upgrade() {
            view.update(cx, |this, cx| {
              let new_settings =
                AudioSettings::from_preset(&Preset::builtins()[preset_idx]);
              let mut state = this.state.lock().unwrap();
              if let Some(item) =
                state.queue.iter_mut().find(|item| item.id == id)
              {
                item.preset_index = preset_idx;
                item.settings = new_settings.clone();
              }
              drop(state);

              // Rebuild inputs for the new preset's track layout
              this.remove_inputs(id);
              this.ensure_input_states(id, &new_settings, window, cx);
              cx.notify();
            });
          }
        }),
      );
    }

    // Export button
    let settings_for_export = item_settings.clone();
    let export_btn = Button::new(SharedString::from(format!("export_{}", id)))
      .icon(IconName::ExternalLink)
      .compact()
      .tooltip("Export Preset")
      .disabled(controls_disabled)
      .on_click(move |_, _, cx| {
        let ini_content = settings_for_export.to_ini();
        cx.spawn(|_: &mut AsyncApp| async move {
          let file = FileDialog::new()
            .add_filter("INI files", &["ini"])
            .set_file_name("preset.ini")
            .save_file();
          if let Some(path) = file {
            let _ = fs::write(path, ini_content);
          }
        })
        .detach();
      });

    // Import button
    let item_view_import = item_view.clone();
    let import_btn = Button::new(SharedString::from(format!("import_{}", id)))
      .icon(IconName::FolderOpen)
      .compact()
      .tooltip("Import Preset")
      .disabled(controls_disabled)
      .on_click(move |_, _, cx| {
        let view = item_view_import.clone();
        cx.spawn(move |cx: &mut AsyncApp| {
          let cx = cx.clone();
          async move {
            let file = FileDialog::new()
              .add_filter("INI files", &["ini"])
              .pick_file();
            let Some(path) = file else { return };
            let Ok(content) = fs::read_to_string(&path) else {
              return;
            };
            let Ok(new_settings) = AudioSettings::from_ini(&content) else {
              return;
            };
            cx.update(|cx| {
              if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| {
                  {
                    let mut state = this.state.lock().unwrap();
                    if let Some(item) =
                      state.queue.iter_mut().find(|item| item.id == id)
                    {
                      item.settings = new_settings;
                    }
                  }
                  // Invalidate inputs and collapse panel so they are recreated on next expand
                  this.remove_inputs(id);
                  if this.expanded_job_id == Some(id) {
                    this.expanded_job_id = None;
                  }
                  cx.notify();
                });
              }
            });
          }
        })
        .detach();
      });

    presets.child(export_btn).child(import_btn)
  }

  fn render_track_list(
    &self,
    id: usize,
    item_settings: &AudioSettings,
    controls_disabled: bool,
    inputs: &[Entity<InputState>],
    item_view: &WeakEntity<Self>,
  ) -> impl IntoElement {
    let track_count = item_settings.tracks.len();
    let mut tracks_container = v_flex().gap_1();
    for (track_idx, track_config) in item_settings.tracks.iter().enumerate() {
      let is_first = track_idx == 0;
      let is_last = track_idx == track_count - 1;
      let item_view_track_up = item_view.clone();
      let item_view_track_down = item_view.clone();

      let mut row = h_flex().gap_2().items_center().child(
        div().w(px(180.0)).text_xs().child(format!(
          "{}. {} track offset (dB):",
          track_idx, &*track_config.name,
        )),
      );

      if let Some(track_input) = inputs.get(track_idx) {
        let blur_on_click_outside = blur_on_click_outside(track_input.clone());
        row = row.child(
          div()
            .id(SharedString::from(format!(
              "track_input_{}_{}",
              id, track_idx
            )))
            .w(px(50.0))
            .child(Input::new(track_input).disabled(controls_disabled))
            .on_mouse_down_out(blur_on_click_outside),
        );
      }

      row = row
        .child(
          Button::new(SharedString::from(format!(
            "track_up_{}_{}",
            id, track_idx
          )))
          .icon(IconName::ArrowUp)
          .compact()
          .tooltip("Move Track Up")
          .disabled(controls_disabled || is_first)
          .on_click(move |_, window, cx| {
            if track_idx == 0 {
              return;
            }
            if let Some(view) = item_view_track_up.upgrade() {
              view.update(cx, |this, cx| {
                this.swap_tracks(id, track_idx, track_idx - 1, window, cx);
              });
            }
          }),
        )
        .child(
          Button::new(SharedString::from(format!(
            "track_down_{}_{}",
            id, track_idx
          )))
          .icon(IconName::ArrowDown)
          .compact()
          .tooltip("Move Track Down")
          .disabled(controls_disabled || is_last)
          .on_click(move |_, window, cx| {
            if let Some(view) = item_view_track_down.upgrade() {
              view.update(cx, |this, cx| {
                this.swap_tracks(id, track_idx, track_idx + 1, window, cx);
              });
            }
          }),
        );

      tracks_container = tracks_container.child(row);
    }
    tracks_container
  }

  fn render_queue_item_settings(
    &self,
    item: &QueueItem,
    is_expanded: bool,
    is_running: bool,
    item_view: &WeakEntity<Self>,
    cx: &mut Context<Self>,
  ) -> Option<AnyElement> {
    if !is_expanded {
      return None;
    }

    let id = item.id;
    let item_settings = item.settings.clone();
    let item_preset_index = item.preset_index;
    let inputs = self
      .get_inputs(id)
      .expect("inputs should exist for expanded item");
    let single_track = item_settings.single_track;
    let controls_disabled = is_running || !item.status.is_pending();

    let preset_row = self.render_preset_row(
      id,
      item_preset_index,
      &item_settings,
      controls_disabled,
      item_view,
    );

    let tracks_container = self.render_track_list(
      id,
      &item_settings,
      controls_disabled,
      inputs,
      item_view,
    );

    let item_view_cb = item_view.clone();

    Some(
      v_flex()
        .mt_2()
        .gap_2()
        .p_2()
        .rounded_sm()
        .bg(cx.theme().accent.opacity(0.02))
        .child(preset_row)
        .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
        .child(
          Checkbox::new(("single_track", id))
            .checked(single_track)
            .label("Single Audio Track (Loudnorm Only)")
            .disabled(controls_disabled)
            .on_click(move |checked, _, cx| {
              if let Some(view) = item_view_cb.upgrade() {
                view.update(cx, |this, cx| {
                  let mut state = this.state.lock().unwrap();
                  if let Some(item) =
                    state.queue.iter_mut().find(|item| item.id == id)
                  {
                    item.settings.single_track = *checked;
                  }
                  cx.notify();
                });
              }
            }),
        )
        .when(!single_track, |this| this.child(tracks_container))
        .into_any_element(),
    )
  }

  fn render_queue_item_status(
    &self,
    item: &QueueItem,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    match &item.status {
      QueueItemStatus::Pending => div()
        .text_color(cx.theme().warning)
        .text_xs()
        .child("Pending"),
      QueueItemStatus::Processing { step, percent, .. } => div()
        .text_color(cx.theme().info)
        .text_xs()
        .child(format!("{} ({:.0}%)", &**step, percent * 100.0)),
      QueueItemStatus::Completed { .. } => div()
        .text_color(cx.theme().success)
        .text_xs()
        .child("Completed"),
      QueueItemStatus::Failed(_) => div()
        .text_color(cx.theme().danger)
        .text_xs()
        .child("Failed"),
      QueueItemStatus::Cancelled => div()
        .text_color(cx.theme().muted_foreground)
        .text_xs()
        .child("Cancelled"),
    }
  }

  fn render_queue_item_header_actions(
    &self,
    item: &QueueItem,
    is_expanded: bool,
    is_first: bool,
    is_last: bool,
    item_view: &WeakEntity<Self>,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let id = item.id;
    let item_settings = item.settings.clone();
    let is_pending = item.status.is_pending();

    let view_for_up = item_view.clone();
    let view_for_down = item_view.clone();
    let view_for_toggle = item_view.clone();
    let view_for_remove = item_view.clone();

    h_flex()
      .gap_2()
      .items_center()
      .child(self.render_queue_item_status(item, cx))
      .when(is_pending, |this| {
        this
          .child(
            Button::new(("up", id))
              .icon(IconName::ArrowUp)
              .compact()
              .tooltip("Move Up")
              .disabled(is_first)
              .on_click(move |_, _, cx| {
                if let Some(view) = view_for_up.upgrade() {
                  view.update(cx, |this, cx| {
                    this.state.lock().unwrap().move_item(id, -1);
                    cx.notify();
                  });
                }
              }),
          )
          .child(
            Button::new(("down", id))
              .icon(IconName::ArrowDown)
              .compact()
              .tooltip("Move Down")
              .disabled(is_last)
              .on_click(move |_, _, cx| {
                if let Some(view) = view_for_down.upgrade() {
                  view.update(cx, |this, cx| {
                    this.state.lock().unwrap().move_item(id, 1);
                    cx.notify();
                  });
                }
              }),
          )
      })
      .child(
        Button::new(("toggle_settings", id))
          .icon(if is_expanded {
            IconName::ChevronUp
          } else {
            IconName::Settings
          })
          .compact()
          .tooltip(if is_expanded {
            "Collapse Settings"
          } else {
            "Settings"
          })
          .on_click(move |_, window, cx| {
            if let Some(view) = view_for_toggle.upgrade() {
              view.update(cx, |this, cx| {
                if this.expanded_job_id == Some(id) {
                  this.expanded_job_id = None;
                } else {
                  let settings = item_settings.clone();
                  this.ensure_input_states(id, &settings, window, cx);
                  this.expanded_job_id = Some(id);
                }
                cx.notify();
              });
            }
          }),
      )
      .child(
        Button::new(("remove", id))
          .danger()
          .icon(IconName::Delete)
          .compact()
          .tooltip("Delete")
          .on_click(move |_, _, cx| {
            if let Some(view) = view_for_remove.upgrade() {
              let is_active = {
                let state = view.read(cx).state.lock().unwrap();
                state.active_processes.iter().any(|(pid, _)| *pid == id)
              };

              let should_remove = if is_active {
                confirm_action(
                  "Confirm Delete",
                  "This video is currently being rendered. Are you sure you want to delete it?",
                )
              } else {
                true
              };

              if should_remove {
                view.update(cx, |this, cx| {
                  this.state.lock().unwrap().remove_item(id);
                  if this.selected_job_id == Some(id) {
                    this.selected_job_id = None;
                  }
                  if this.expanded_job_id == Some(id) {
                    this.expanded_job_id = None;
                  }
                  this.remove_inputs(id);
                  cx.notify();
                });
              }
            }
          }),
      )
  }

  fn render_queue_item_progress(
    &self,
    item: &QueueItem,
    cx: &mut Context<Self>,
  ) -> Option<AnyElement> {
    match &item.status {
      QueueItemStatus::Processing {
        percent,
        speed,
        time_str,
        ..
      } => {
        let percent = *percent;
        let speed = Arc::clone(speed);
        let time_str = Arc::clone(time_str);
        Some(
          v_flex()
            .mt_2()
            .gap_1()
            .child(
              Progress::new(("progress", item.id))
                .bg(cx.theme().info)
                .value(percent * 100.0),
            )
            .child(
              h_flex()
                .justify_end()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                  h_flex()
                    .gap_2()
                    .when(!speed.is_empty(), |this| {
                      this.child(format!("Speed: {}", &*speed))
                    })
                    .when(!time_str.is_empty(), |this| {
                      this.child(format!("Time: {}", &*time_str))
                    }),
                ),
            )
            .into_any_element(),
        )
      }
      _ => None,
    }
  }
}
