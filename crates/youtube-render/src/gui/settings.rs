use gpui::{
  div, prelude::*, px, Context, IntoElement, ParentElement, Styled, WeakEntity,
};
use gpui_component::{
  checkbox::Checkbox, h_flex, input::Input, v_flex, Disableable,
};

use crate::gui::YtRenderApp;

impl YtRenderApp {
  pub(super) fn render_settings_panel(
    &self,
    is_running: bool,
    enable_parallel: bool,
    view: &WeakEntity<Self>,
    _cx: &mut Context<Self>,
  ) -> impl IntoElement {
    v_flex()
      .gap_4()
      .flex_grow()
      .h_full()
      .child(
        v_flex().gap_2().child(
          h_flex()
            .gap_4()
            .items_center()
            .child(div().child("ffmpeg executable path:"))
            .child(
              div().flex_grow().child(
                Input::new(&self.ffmpeg_path_state).disabled(is_running),
              ),
            ),
        ),
      )
      .child(
        h_flex()
          .gap_4()
          .items_center()
          .child(
            Checkbox::new("enable_parallel")
              .checked(enable_parallel)
              .label("Enable parallel rendering")
              .disabled(is_running)
              .on_click({
                let view = view.clone();
                move |checked, _, cx| {
                  if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                      this.state.lock().unwrap().enable_parallel = *checked;
                      cx.notify();
                    });
                  }
                }
              }),
          )
          .child(h_flex().gap_2().items_center().when(
            enable_parallel,
            |this| {
              this.child(div().child("Parallel jobs:")).child(
                div().w(px(60.0)).child(
                  Input::new(&self.parallel_jobs_state).disabled(is_running),
                ),
              )
            },
          )),
      )
  }
}
