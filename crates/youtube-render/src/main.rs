use std::{
  panic::{set_hook, take_hook},
  process::exit,
};

use gpui::{
  px, size, AppContext, Application, Bounds, TitlebarOptions, WindowBounds,
  WindowOptions,
};
use gpui_component::{init as init_gpui_component, Root};
use youtube_render::{
  ffmpeg::kill_all_children,
  gui::{confirm_quit, set_active_app_state, YtRenderApp},
  EmbedAssets,
};

fn main() {
  // Set up panic hook to ensure spawned children are killed if we panic
  let default_hook = take_hook();
  set_hook(Box::new(move |info| {
    kill_all_children();
    default_hook(info);
  }));

  // Set up ctrlc handler to ensure cleanup on termination signals
  let _ = ctrlc::set_handler(move || {
    if confirm_quit() {
      kill_all_children();
      exit(130);
    }
  });

  let app = Application::new().with_assets(EmbedAssets);
  app.run(move |cx| {
    init_gpui_component(cx);

    let bounds = Bounds::centered(None, size(px(1280.0), px(720.0)), cx);
    let options = WindowOptions {
      window_bounds: Some(WindowBounds::Windowed(bounds)),
      window_min_size: Some(size(px(1280.0), px(720.0))),
      titlebar: Some(TitlebarOptions {
        title: Some("YouTube Video Renderer".into()),
        ..Default::default()
      }),
      ..Default::default()
    };

    cx.open_window(options, |window, cx| {
      let view = cx.new(|cx| YtRenderApp::new(window, cx));
      let state_arc = view.read(cx).state();
      set_active_app_state(state_arc.clone());

      window.on_window_should_close(cx, move |_, _| confirm_quit());

      cx.new(|cx| Root::new(view, window, cx))
    })
    .unwrap();
    cx.activate(true);
  });

  // Clean up any remaining children on clean termination/exit
  kill_all_children();
}
