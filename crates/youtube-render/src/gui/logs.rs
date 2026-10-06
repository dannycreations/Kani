use std::sync::Arc;

use gpui_kit::{
  component::{scroll::ScrollableElement as _, v_flex, ActiveTheme},
  div,
  prelude::*,
  px, Context, FontWeight, IntoElement, ParentElement, Styled,
};

use crate::{
  core::queue::{AppState, QueueItemStatus},
  gui::RenderApp,
};

impl RenderApp {
  pub(super) fn render_logs_panel(
    &self,
    selected_job_id: Option<usize>,
    state: &AppState,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let target_id = selected_job_id
      .or_else(|| state.active_processes.first().map(|(id, _)| *id));

    let display_job = target_id
      .and_then(|id| state.queue.iter().find(|item| item.id == id).cloned());

    v_flex()
      .gap_2()
      .h_full()
      .child(
        div()
          .font_weight(FontWeight::BOLD)
          .text_lg()
          .child("Job Logs"),
      )
      .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
      .child(
        if let Some(job) = display_job {
          v_flex()
            .gap_2()
            .flex_grow(1.0)
            .h_full()
            .when_some(
              match &job.status {
                QueueItemStatus::Failed(err) => Some(Arc::clone(err)),
                _ => None,
              },
              |this, err| {
                this.child(
                  div()
                    .text_color(cx.theme().danger)
                    .text_sm()
                    .child(format!("Error: {}", &*err)),
                )
              },
            )
            .child(
              v_flex()
                .flex_grow(1.0)
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
            .flex_grow(1.0)
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
      )
  }
}
