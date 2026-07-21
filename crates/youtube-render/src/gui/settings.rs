use gpui::{
  div, prelude::*, px, Focusable, IntoElement, ParentElement, Styled,
  WeakEntity,
};
use gpui_component::{
  checkbox::Checkbox, h_flex, input::Input, v_flex, Disableable,
};

use crate::gui::RenderApp;

impl RenderApp {
  pub(super) fn render_settings_panel(
    &self,
    is_running: bool,
    enable_parallel: bool,
    view: &WeakEntity<Self>,
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
              div()
                .id("ffmpeg_path_input_wrapper")
                .flex_grow()
                .child(Input::new(&self.ffmpeg_path_state).disabled(is_running))
                .on_mouse_down_out({
                  let state = self.ffmpeg_path_state.clone();
                  move |_, window, cx| {
                    if state.read(cx).focus_handle(cx).is_focused(window) {
                      state.update(cx, |input, cx| {
                        input.unselect(window, cx);
                      });
                      window.blur();
                    }
                  }
                }),
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
                div()
                  .id("parallel_jobs_input_wrapper")
                  .w(px(60.0))
                  .child(
                    Input::new(&self.parallel_jobs_state).disabled(is_running),
                  )
                  .on_mouse_down_out({
                    let state = self.parallel_jobs_state.clone();
                    move |_, window, cx| {
                      if state.read(cx).focus_handle(cx).is_focused(window) {
                        state.update(cx, |input, cx| {
                          input.unselect(window, cx);
                        });
                        window.blur();
                      }
                    }
                  }),
              )
            },
          )),
      )
  }
}
