use std::{
  collections::{HashMap, HashSet},
  path::Path,
  sync::{Arc, Mutex},
  time::Duration,
};

use gpui::{
  div, prelude::*, px, AsyncApp, Context, Entity, FontWeight,
  InteractiveElement, IntoElement, ParentElement, Render, Styled, Timer,
  Window,
};
use gpui_component::{
  button::{Button, ButtonVariants as _},
  checkbox::Checkbox,
  divider::Divider,
  group_box::{GroupBox, GroupBoxVariants as _},
  h_flex,
  input::{Input, InputEvent, InputState},
  progress::Progress,
  scroll::ScrollableElement as _,
  slider::{Slider, SliderEvent, SliderState, SliderValue},
  tab::{Tab, TabBar},
  theme::ActiveTheme as _,
  v_flex, Disableable as _, IconName, Selectable,
};
use rfd::FileDialog;

use crate::{
  ffmpeg::{kill_all_children, AudioSettings, AudioTrack},
  queue::{AppState, QueueItemStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTab {
  Global,
}

pub struct TrackSliderState {
  pub slider_state: Entity<SliderState>,
  pub input_state: Entity<InputState>,
}

pub struct ItemSliderStates {
  pub tracks: HashMap<AudioTrack, TrackSliderState>,
}

pub struct YtRenderApp {
  state: Arc<Mutex<AppState>>,
  selected_job_id: Option<usize>,
  expanded_job_id: Option<usize>,
  active_tab: ConfigTab,

  // Per-video config slider states mapped by job ID
  item_sliders: HashMap<usize, ItemSliderStates>,

  // Track which item sliders have their layout bounds resolved to avoid blinks
  sliders_ready: HashSet<usize>,

  // Global config input state
  ffmpeg_path_state: Entity<InputState>,
}

impl YtRenderApp {
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

    Self {
      state,
      selected_job_id: None,
      expanded_job_id: None,
      active_tab: ConfigTab::Global,
      item_sliders: HashMap::new(),
      sliders_ready: HashSet::new(),
      ffmpeg_path_state,
    }
  }

  fn ensure_slider_states(
    &mut self,
    id: usize,
    settings: &AudioSettings,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if self.item_sliders.contains_key(&id) {
      return;
    }

    let mut tracks_map = HashMap::new();

    for &track in AudioTrack::all() {
      let val = settings.get_offset(track);
      let slider_state = cx.new(|_| {
        SliderState::new()
          .min(-30.0)
          .max(0.0)
          .step(1.0)
          .default_value(val)
      });
      let input_state = cx.new(|cx| {
        InputState::new(window, cx).default_value(format!("{:.0}", val))
      });

      // Subscribe to slider changes
      cx.subscribe_in(
        &slider_state,
        window,
        move |this, _, event, window, cx| {
          if let SliderEvent::Change(SliderValue::Single(val)) = event {
            let mut val = *val;
            if val == 0.0 {
              val = 0.0;
            }
            let mut state = this.state.lock().unwrap();
            if let Some(item) =
              state.queue.iter_mut().find(|item| item.id == id)
            {
              item.settings.set_offset(track, val);
            }
            if let Some(sliders) = this.item_sliders.get(&id) {
              if let Some(track_slider) = sliders.tracks.get(&track) {
                let input_val = track_slider.input_state.read(cx).value();
                let needs_update = match input_val.parse::<f32>() {
                  Ok(parsed) => {
                    let mut parsed_norm = parsed;
                    if parsed_norm == 0.0 {
                      parsed_norm = 0.0;
                    }
                    (parsed_norm - val).abs() > 0.001
                  }
                  Err(_) => true,
                };
                if needs_update {
                  track_slider.input_state.update(cx, |input, cx| {
                    input.set_value(format!("{:.0}", val), window, cx);
                  });
                }
              }
            }
            cx.notify();
          }
        },
      )
      .detach();

      // Subscribe to input changes/blur
      cx.subscribe_in(
        &input_state,
        window,
        move |this, input_entity, event, window, cx| match event {
          InputEvent::Change => {
            let val_str = input_entity.read(cx).value();
            if let Ok(val) = val_str.parse::<f32>() {
              let mut clamped_val = val.clamp(-30.0, 0.0);
              if clamped_val == 0.0 {
                clamped_val = 0.0;
              }
              let mut state = this.state.lock().unwrap();
              if let Some(item) =
                state.queue.iter_mut().find(|item| item.id == id)
              {
                item.settings.set_offset(track, clamped_val);
              }
              if let Some(sliders) = this.item_sliders.get(&id) {
                if let Some(track_slider) = sliders.tracks.get(&track) {
                  let mut slider_val =
                    track_slider.slider_state.read(cx).value().start();
                  if slider_val == 0.0 {
                    slider_val = 0.0;
                  }
                  if (slider_val - clamped_val).abs() > 0.001 {
                    track_slider.slider_state.update(cx, |slider, cx| {
                      slider.set_value(clamped_val, window, cx);
                    });
                  }
                }
              }
              cx.notify();
            }
          }
          InputEvent::Blur | InputEvent::PressEnter { .. } => {
            let mut current_offset = track.default_offset();
            {
              let state = this.state.lock().unwrap();
              if let Some(item) = state.queue.iter().find(|item| item.id == id)
              {
                current_offset = item.settings.get_offset(track);
              }
            }
            if current_offset == 0.0 {
              current_offset = 0.0;
            }
            if let Some(sliders) = this.item_sliders.get(&id) {
              if let Some(track_slider) = sliders.tracks.get(&track) {
                track_slider.input_state.update(cx, |input, cx| {
                  input.set_value(format!("{:.0}", current_offset), window, cx);
                });
              }
            }
            cx.notify();
          }
          _ => {}
        },
      )
      .detach();

      tracks_map.insert(
        track,
        TrackSliderState {
          slider_state,
          input_state,
        },
      );
    }

    self
      .item_sliders
      .insert(id, ItemSliderStates { tracks: tracks_map });
  }
}

impl Drop for YtRenderApp {
  fn drop(&mut self) {
    kill_all_children();
  }
}

impl Render for YtRenderApp {
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

    // Render configuration settings tabs
    let config_tab_bar = TabBar::new("config_tabs").underline().child(
      Tab::new()
        .label("Global Settings")
        .selected(self.active_tab == ConfigTab::Global)
        .on_click({
          let view = view.clone();
          move |_, _, cx| {
            if let Some(view) = view.upgrade() {
              view.update(cx, |this, cx| {
                this.active_tab = ConfigTab::Global;
                cx.notify();
              });
            }
          }
        }),
    );

    let config_content = match self.active_tab {
      ConfigTab::Global => v_flex().gap_2().child(
        h_flex()
          .gap_4()
          .items_center()
          .child(div().child("ffmpeg executable path:"))
          .child(
            div()
              .flex_grow()
              .child(Input::new(&self.ffmpeg_path_state).disabled(is_running)),
          ),
      ),
    };

    let top_panel = v_flex().child(
      GroupBox::new()
        .outline()
        .title(
          h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .child("Configuration Settings")
            .child(if is_running {
              div().text_color(cx.theme().info).child("Running")
            } else {
              div().text_color(cx.theme().muted_foreground).child("Idle")
            }),
        )
        .child(v_flex().gap_4().child(config_tab_bar).child(config_content)),
    );

    let start_stop_btn = if is_running {
      Button::new("stop").danger().label("Stop").on_click({
        let view = view.clone();
        move |_, _, cx| {
          if let Some(view) = view.upgrade() {
            view.update(cx, |this, cx| {
              this.state.lock().unwrap().stop();
              cx.notify();
            });
          }
        }
      })
    } else {
      let has_pending = state
        .queue
        .iter()
        .any(|item| matches!(item.status, QueueItemStatus::Pending));
      Button::new("start")
        .primary()
        .label("Start")
        .disabled(!has_pending)
        .on_click({
          let view = view.clone();
          move |_, _, cx| {
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
                      let is_running = state_clone.lock().unwrap().is_running;
                      if !is_running {
                        let _ = cx.update(|cx| cx.refresh_windows());
                        break;
                      }
                      let _ = cx.update(|cx| cx.refresh_windows());
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

    let add_files_btn =
      Button::new("add_files").label("Add Video Files").on_click({
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

    let clear_completed_btn = Button::new("clear_completed")
      .label("Clear Completed")
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
              this.item_sliders.retain(|id, _| queue_ids.contains(id));
              this.sliders_ready.retain(|id| queue_ids.contains(id));
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

    let clear_all_btn = Button::new("clear_all").label("Clear All").on_click({
      let view = view.clone();
      move |_, _, cx| {
        if let Some(view) = view.upgrade() {
          view.update(cx, |this, cx| {
            this.state.lock().unwrap().clear_all();
            this.item_sliders.clear();
            this.sliders_ready.clear();
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
      for item in &state.queue {
        let is_selected = self.selected_job_id == Some(item.id);
        let is_expanded = self.expanded_job_id == Some(item.id);
        let id = item.id;
        let item_settings = item.settings.clone();

        let filename = Path::new(&*item.input_path)
          .file_name()
          .and_then(|f| f.to_str())
          .unwrap_or(&item.input_path)
          .to_string();

        let item_view = view.clone();
        let state_arc_up = Arc::clone(&self.state);
        let state_arc_down = Arc::clone(&self.state);

        let item_settings_panel = if is_expanded {
          let sliders = self.item_sliders.get(&id).unwrap();
          let single_track = item_settings.single_track;
          let sliders_disabled =
            is_running || !matches!(item.status, QueueItemStatus::Pending);
          let is_ready = self.sliders_ready.contains(&id);
          let slider_opacity = if is_ready { 1.0 } else { 0.0 };

          let mut tracks_container = v_flex().gap_1();
          for &track in AudioTrack::all() {
            if let Some(track_slider) = sliders.tracks.get(&track) {
              tracks_container = tracks_container.child(
                h_flex()
                  .gap_2()
                  .items_center()
                  .child(div().w(px(180.0)).text_xs().child(format!(
                    "{} track offset (dB):",
                    track.display_name()
                  )))
                  .child(
                    div().flex_grow().child(
                      Slider::new(&track_slider.slider_state)
                        .disabled(sliders_disabled)
                        .opacity(slider_opacity),
                    ),
                  )
                  .child(
                    div().w(px(50.0)).child(
                      Input::new(&track_slider.input_state)
                        .disabled(sliders_disabled),
                    ),
                  ),
              );
            }
          }

          Some(
            v_flex()
              .mt_2()
              .gap_2()
              .p_2()
              .rounded_sm()
              .bg(cx.theme().accent.opacity(0.02))
              .child(
                Checkbox::new(("single_track", id))
                  .checked(single_track)
                  .label("Single Audio Track (Loudnorm Only)")
                  .disabled(sliders_disabled)
                  .on_click({
                    let item_view = item_view.clone();
                    move |checked, _, cx| {
                      if let Some(view) = item_view.upgrade() {
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
                    }
                  }),
              )
              .when(!single_track, |this| this.child(tracks_container)),
          )
        } else {
          None
        };

        let queue_item_el = v_flex()
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
                  .child(filename.clone())
                  .on_click({
                    let item_view = item_view.clone();
                    move |_, _, cx| {
                      if let Some(view) = item_view.upgrade() {
                        view.update(cx, |this, cx| {
                          this.selected_job_id = Some(id);
                          cx.notify();
                        });
                      }
                    }
                  }),
              )
              .child(
                h_flex()
                  .gap_2()
                  .items_center()
                  .child(match &item.status {
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
                  })
                  .when(
                    matches!(item.status, QueueItemStatus::Pending),
                    |this| {
                      let item_view_up = item_view.clone();
                      let item_view_down = item_view.clone();
                      let state_arc_up = state_arc_up.clone();
                      let state_arc_down = state_arc_down.clone();
                      this
                        .child(
                          Button::new(("up", id))
                            .icon(IconName::ArrowUp)
                            .compact()
                            .on_click(move |_, _, cx| {
                              state_arc_up.lock().unwrap().move_up(id);
                              if let Some(view) = item_view_up.upgrade() {
                                view.update(cx, |_, cx| cx.notify());
                              }
                            }),
                        )
                        .child(
                          Button::new(("down", id))
                            .icon(IconName::ArrowDown)
                            .compact()
                            .on_click(move |_, _, cx| {
                              state_arc_down.lock().unwrap().move_down(id);
                              if let Some(view) = item_view_down.upgrade() {
                                view.update(cx, |_, cx| cx.notify());
                              }
                            }),
                        )
                    },
                  )
                  .child(
                    Button::new(("toggle_settings", id))
                      .icon(if is_expanded {
                        IconName::ChevronUp
                      } else {
                        IconName::Settings
                      })
                      .compact()
                      .on_click({
                        let item_view = item_view.clone();
                        let item_settings = item_settings.clone();
                        move |_, window, cx| {
                          if let Some(view) = item_view.upgrade() {
                            let mut is_expanding = false;
                            view.update(cx, |this, cx| {
                              if this.expanded_job_id == Some(id) {
                                this.expanded_job_id = None;
                              } else {
                                let settings = item_settings.clone();
                                this.ensure_slider_states(
                                  id, &settings, window, cx,
                                );
                                this.expanded_job_id = Some(id);
                                is_expanding = true;
                              }
                              cx.notify();
                            });

                            if is_expanding {
                              let has_ready = view.update(cx, |this, _| {
                                this.sliders_ready.contains(&id)
                              });

                              if !has_ready {
                                let view_clone = item_view.clone();
                                cx.spawn(move |app: &mut AsyncApp| {
                                  let app = app.clone();
                                  let view_clone = view_clone.clone();
                                  async move {
                                    Timer::after(Duration::from_millis(50))
                                      .await;
                                    let _ = app.update(|cx| {
                                      if let Some(view) = view_clone.upgrade() {
                                        view.update(cx, |this, cx| {
                                          this.sliders_ready.insert(id);
                                          cx.notify();
                                        });
                                      }
                                    });
                                  }
                                })
                                .detach();
                              }
                            }
                          }
                        }
                      }),
                  )
                  .child(
                    Button::new(("remove", id))
                      .danger()
                      .icon(IconName::Delete)
                      .compact()
                      .on_click({
                        let item_view = item_view.clone();
                        move |_, _, cx| {
                          if let Some(view) = item_view.upgrade() {
                            view.update(cx, |this, cx| {
                              this.state.lock().unwrap().remove_item(id);
                              if this.selected_job_id == Some(id) {
                                this.selected_job_id = None;
                              }
                              if this.expanded_job_id == Some(id) {
                                this.expanded_job_id = None;
                              }
                              this.item_sliders.remove(&id);
                              this.sliders_ready.remove(&id);
                              cx.notify();
                            });
                          }
                        }
                      }),
                  ),
              ),
          )
          .when_some(item_settings_panel, |this, panel| this.child(panel))
          .when_some(
            match &item.status {
              QueueItemStatus::Processing {
                percent,
                speed,
                time_str,
                ..
              } => Some((*percent, Arc::clone(speed), Arc::clone(time_str))),
              _ => None,
            },
            |this, (percent, speed, time_str)| {
              this.child(
                v_flex()
                  .mt_2()
                  .gap_1()
                  .child(
                    Progress::new().bg(cx.theme().info).value(percent * 100.0),
                  )
                  .child(
                    h_flex()
                      .justify_between()
                      .text_xs()
                      .text_color(cx.theme().muted_foreground)
                      .child(format!("{:.0}%", percent * 100.0))
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
                  ),
              )
            },
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
          .child("Render Queue"),
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

    let target_id = self.selected_job_id.or(state.current_job_id);
    let display_job = target_id
      .and_then(|id| state.queue.iter().find(|item| item.id == id).cloned());

    let right_col = v_flex()
      .gap_2()
      .h_full()
      .child(
        div()
          .font_weight(FontWeight::BOLD)
          .text_lg()
          .child("Job Logs & Output Info"),
      )
      .child(Divider::horizontal())
      .child(
        if let Some(job) = display_job {
          let output_exists = Path::new(&*job.output_path).exists();
          v_flex()
            .gap_2()
            .flex_grow()
            .h_full()
            .child(div().text_sm().child(format!("Input: {}", &*job.input_path)))
            .child(
              match &job.status {
                QueueItemStatus::Completed { output_path } => {
                  if output_exists {
                    div().text_color(cx.theme().success).text_sm().child(format!("Output: {}", &**output_path))
                  } else {
                    div().text_sm().child(format!("Output: {}", &**output_path))
                  }
                }
                QueueItemStatus::Failed(err) => {
                  div().text_color(cx.theme().danger).text_sm().child(format!("Error: {}", &**err))
                }
                _ => {
                  div().text_sm().child(format!("Output: {}", &*job.output_path))
                }
              }
            )
            .child(Divider::horizontal())
            .child(div().text_sm().child("Processing logs:"))
            .child(
              v_flex()
                .flex_grow()
                .h_full()
                .overflow_y_scrollbar()
                .bg(cx.theme().accent.opacity(0.05))
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .children(
                  if job.logs.is_empty() {
                    vec![div().text_color(cx.theme().muted_foreground).text_xs().child("No logs yet.").into_any_element()]
                  } else {
                    job.logs
                      .iter()
                      .map(|line| div().text_xs().child(line.to_string()).into_any_element())
                      .collect()
                  }
                )
            )
        } else {
          v_flex()
            .flex_grow()
            .justify_center()
            .items_center()
            .gap_2()
            .child(div().child("No item selected."))
            .child(
              div()
                .text_color(cx.theme().muted_foreground)
                .text_xs()
                .child("Click on an item in the queue to view its logs and output path here.")
            )
        }
      );

    v_flex()
      .p_4()
      .gap_4()
      .size_full()
      .child(top_panel)
      .child(Divider::horizontal())
      .child(if use_two_columns {
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
      })
  }
}
