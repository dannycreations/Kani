use std::{fs, sync::Arc};

use gpui::{
  div, prelude::*, px, AnyElement, AsyncApp, Context, IntoElement,
  ParentElement, SharedString, Styled, WeakEntity, Window,
};
use gpui_component::{
  button::{Button, ButtonVariants as _},
  checkbox::Checkbox,
  divider::Divider,
  h_flex,
  input::Input,
  progress::Progress,
  v_flex, ActiveTheme, Disableable, IconName,
};
use rfd::FileDialog;

use crate::{
  ffmpeg::{AudioSettings, Preset},
  gui::{ItemInputStates, YtRenderApp},
  queue::{QueueItem, QueueItemStatus},
};

impl YtRenderApp {
  #[allow(clippy::too_many_arguments)]
  pub(super) fn render_queue_item(
    &self,
    item: &QueueItem,
    _item_idx: usize,
    is_selected: bool,
    is_expanded: bool,
    is_running: bool,
    display_name: String,
    view: &WeakEntity<Self>,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let id = item.id;
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
    _cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let builtins = Preset::builtins();
    let mut preset_row = h_flex()
      .gap_2()
      .items_center()
      .child(div().text_xs().child("Preset:"));

    for (preset_idx, preset) in builtins.iter().enumerate() {
      let is_active_preset = current_preset_idx == preset_idx;
      let preset_name = preset.name.to_string();
      let item_view_preset = item_view.clone();
      preset_row = preset_row.child(
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
              {
                let mut state = this.state.lock().unwrap();
                if let Some(item) =
                  state.queue.iter_mut().find(|item| item.id == id)
                {
                  item.preset_index = preset_idx;
                  item.settings =
                    AudioSettings::from_preset(&Preset::builtins()[preset_idx]);
                }
              }
              // Rebuild inputs for the new preset's track layout
              this.remove_inputs(id);
              let new_settings = {
                let state = this.state.lock().unwrap();
                state
                  .queue
                  .iter()
                  .find(|item| item.id == id)
                  .map(|item| item.settings.clone())
              };
              if let Some(settings) = new_settings {
                this.ensure_input_states(id, &settings, window, cx);
              }
              cx.notify();
            });
          }
        }),
      );
    }

    // Export button
    let settings_for_export = item_settings.clone();
    preset_row = preset_row.child(
      Button::new(SharedString::from(format!("export_{}", id)))
        .icon(IconName::ExternalLink)
        .compact()
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
        }),
    );

    // Import button
    let item_view_import = item_view.clone();
    preset_row = preset_row.child(
      Button::new(SharedString::from(format!("import_{}", id)))
        .icon(IconName::FolderOpen)
        .compact()
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
              let _ = cx.update(|cx| {
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
        }),
    );

    preset_row
  }

  fn render_track_list(
    &self,
    id: usize,
    item_settings: &AudioSettings,
    controls_disabled: bool,
    inputs: &ItemInputStates,
    item_view: &WeakEntity<Self>,
    _cx: &mut Context<Self>,
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

      if let Some(track_input) = inputs.tracks.get(track_idx) {
        row = row.child(div().w(px(50.0)).child(
          Input::new(&track_input.input_state).disabled(controls_disabled),
        ));
      }

      row = row
        .child(
          Button::new(SharedString::from(format!(
            "track_up_{}_{}",
            id, track_idx
          )))
          .icon(IconName::ArrowUp)
          .compact()
          .disabled(controls_disabled || is_first)
          .on_click(move |_, window, cx| {
            if let Some(view) = item_view_track_up.upgrade() {
              view.update(cx, |this, cx| {
                {
                  let mut state = this.state.lock().unwrap();
                  if let Some(item) =
                    state.queue.iter_mut().find(|item| item.id == id)
                  {
                    if track_idx > 0 {
                      item.settings.tracks.swap(track_idx, track_idx - 1);
                    }
                  }
                }
                // Rebuild inputs to match the new track order
                this.remove_inputs(id);
                let new_settings = {
                  let state = this.state.lock().unwrap();
                  state
                    .queue
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.settings.clone())
                };
                if let Some(settings) = new_settings {
                  this.ensure_input_states(id, &settings, window, cx);
                }
                cx.notify();
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
          .disabled(controls_disabled || is_last)
          .on_click(move |_, window, cx| {
            if let Some(view) = item_view_track_down.upgrade() {
              view.update(cx, |this, cx| {
                {
                  let mut state = this.state.lock().unwrap();
                  if let Some(item) =
                    state.queue.iter_mut().find(|item| item.id == id)
                  {
                    if track_idx + 1 < item.settings.tracks.len() {
                      item.settings.tracks.swap(track_idx, track_idx + 1);
                    }
                  }
                }
                // Rebuild inputs to match the new track order
                this.remove_inputs(id);
                let new_settings = {
                  let state = this.state.lock().unwrap();
                  state
                    .queue
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.settings.clone())
                };
                if let Some(settings) = new_settings {
                  this.ensure_input_states(id, &settings, window, cx);
                }
                cx.notify();
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
    let controls_disabled =
      is_running || !matches!(item.status, QueueItemStatus::Pending);

    let preset_row = self.render_preset_row(
      id,
      item_preset_index,
      &item_settings,
      controls_disabled,
      item_view,
      cx,
    );

    let tracks_container = self.render_track_list(
      id,
      &item_settings,
      controls_disabled,
      inputs,
      item_view,
      cx,
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
        .child(Divider::horizontal())
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
    item_view: &WeakEntity<Self>,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let id = item.id;
    let item_settings = item.settings.clone();
    let is_pending = matches!(item.status, QueueItemStatus::Pending);

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
              .on_click(move |_, _, cx| {
                if let Some(view) = view_for_up.upgrade() {
                  view.update(cx, |this, cx| {
                    this.state.lock().unwrap().move_up(id);
                    cx.notify();
                  });
                }
              }),
          )
          .child(
            Button::new(("down", id))
              .icon(IconName::ArrowDown)
              .compact()
              .on_click(move |_, _, cx| {
                if let Some(view) = view_for_down.upgrade() {
                  view.update(cx, |this, cx| {
                    this.state.lock().unwrap().move_down(id);
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
          .on_click(move |_, _, cx| {
            if let Some(view) = view_for_remove.upgrade() {
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
            .child(Progress::new().bg(cx.theme().info).value(percent * 100.0))
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
