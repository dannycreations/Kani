use std::sync::Arc;

use gpui::{
  div, prelude::*, Context, FontWeight, IntoElement, ParentElement, Styled,
};
use gpui_component::{
  divider::Divider, scroll::ScrollableElement as _, v_flex, ActiveTheme,
};

use crate::{
  gui::YtRenderApp,
  queue::{AppState, QueueItemStatus},
};

impl YtRenderApp {
  pub(super) fn render_logs_panel(
    &self,
    selected_job_id: Option<usize>,
    state: &AppState,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let target_id = selected_job_id.or_else(|| {
      if state.active_processes.is_empty() {
        None
      } else {
        Some(state.active_processes[0].0)
      }
    });

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
      .child(Divider::horizontal())
      .child(
        if let Some(job) = display_job {
          v_flex()
            .gap_2()
            .flex_grow()
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
      )
  }
}
