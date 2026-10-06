mod core;
mod ffmpeg;
mod gui;

use core::assets::EmbedAssets;
use std::{
  panic::{set_hook, take_hook},
  process::exit,
};

use ffmpeg::kill_all_children;
use gpui_kit::{
  application,
  component::{init as init_gpui_component, Root, Theme, ThemeMode},
  px, size, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};
use gui::{confirm_quit, RenderApp};

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

  let app = application().with_assets(EmbedAssets);
  app.run(move |cx| {
    init_gpui_component(cx);
    Theme::change(ThemeMode::Dark, None, cx);

    let window_size = size(px(1280.0), px(720.0));
    let bounds = Bounds::centered(None, window_size, cx);
    let options = WindowOptions {
      window_bounds: Some(WindowBounds::Windowed(bounds)),
      window_min_size: Some(window_size),
      titlebar: Some(TitlebarOptions {
        title: Some("YouTube Video Renderer".into()),
        ..Default::default()
      }),
      ..Default::default()
    };

    cx.open_window(options, |window, cx| {
      let view = cx.new(|cx| RenderApp::new(window, cx));

      window.on_window_should_close(cx, move |_, _| confirm_quit());

      cx.new(|cx| Root::new(view, window, cx))
    })
    .unwrap();
    cx.activate(true);
  });

  // Clean up any remaining children on clean termination/exit
  kill_all_children();
}
