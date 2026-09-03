#![cfg_attr(not(test), windows_subsystem = "windows")]

mod config;
mod enforcer;
mod instance;
mod registry;
mod tray;
mod utils;

use std::{
  fs,
  process::Command,
  sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc::channel,
    Arc,
  },
  thread,
  time::Duration,
};

use anyhow::Result;
use clap::Parser;
use config::Config;
#[cfg(windows)]
use enforcer::{AudioEnforcer, AudioFlow, EnforcerEvent};
#[cfg(windows)]
use instance::acquire_single_instance_guard;
#[cfg(windows)]
use registry::{deregister_autorun, is_autorun_registered, register_autorun};
#[cfg(windows)]
use tray::{TrayAction, TrayApp};
#[cfg(windows)]
use windows::{
  Win32::Foundation::{LPARAM, WPARAM},
  Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
  },
  Win32::System::ProcessStatus::EmptyWorkingSet,
  Win32::System::Threading::{GetCurrentProcess, GetCurrentThreadId},
  Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, MSG, WM_QUIT, WM_USER,
  },
};

pub const WM_WAKEUP: u32 = WM_USER + 1;

/// Lock default input and output volumes at fixed target levels.
#[derive(Parser, Debug)]
#[command(
  name = "volume-level-lock",
  about = "Locks input and output volume levels"
)]
struct Args {
  /// Level to lock both input and output volume at (1-100)
  #[arg(short, long)]
  level: Option<u32>,

  /// Level to lock input volume at (1-100)
  #[arg(short = 'i', long)]
  input_level: Option<u32>,

  /// Level to lock output volume at (1-100)
  #[arg(short = 'o', long)]
  output_level: Option<u32>,

  /// Install application to autorun registry
  #[arg(long)]
  install: bool,

  /// Uninstall application from autorun registry
  #[arg(long)]
  uninstall: bool,

  /// Start in background hidden mode
  #[arg(long)]
  hidden: bool,
}

fn main() -> Result<()> {
  let args = Args::parse();

  #[cfg(not(windows))]
  {
    println!("This application is only supported on Windows.");
    return Ok(());
  }

  #[cfg(windows)]
  run_windows(args)
}

#[cfg(windows)]
fn run_windows(args: Args) -> Result<()> {
  // 1. Single Instance Check via Named Mutex
  let _guard = match acquire_single_instance_guard()? {
    Some(guard) => guard,
    None => return Ok(()), // Quietly exit if already running
  };

  proceed(args)
}

#[cfg(windows)]
fn proceed(args: Args) -> Result<()> {
  // Load config/settings
  let mut config = Config::load()?;

  if let Some(target) = args.level {
    config.input_target = target.clamp(1, 100);
    config.output_target = target.clamp(1, 100);
    config.save()?;
  }

  if let Some(target) = args.input_level {
    config.input_target = target.clamp(1, 100);
    config.save()?;
  }

  if let Some(target) = args.output_level {
    config.output_target = target.clamp(1, 100);
    config.save()?;
  }

  if args.install {
    register_autorun()?;
    return Ok(());
  }

  if args.uninstall {
    deregister_autorun()?;
    return Ok(());
  }

  // Run the main audio lock enforcer
  run_enforcer(config)?;

  Ok(())
}

#[cfg(windows)]
fn run_enforcer(config: Config) -> Result<()> {
  unsafe {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
  }

  let input_target = Arc::new(AtomicU32::new(config.input_target));
  let output_target = Arc::new(AtomicU32::new(config.output_target));
  let input_paused = Arc::new(AtomicBool::new(config.input_paused));
  let output_paused = Arc::new(AtomicBool::new(config.output_paused));
  let main_thread_id = unsafe { GetCurrentThreadId() };

  // Setup System Tray icon
  let tray_app = TrayApp::new(
    config.input_target,
    config.input_paused,
    config.output_target,
    config.output_paused,
    main_thread_id,
  )?;

  // Setup CTRL-C handler to exit cleanly
  let main_thread_id_clone = main_thread_id;
  ctrlc::set_handler(move || unsafe {
    let _ =
      PostThreadMessageW(main_thread_id_clone, WM_QUIT, WPARAM(0), LPARAM(0));
  })?;

  // Setup working set trimmer thread
  let trimmer_running = Arc::new(AtomicBool::new(true));
  let trimmer_running_clone = trimmer_running.clone();
  thread::spawn(move || {
    while trimmer_running_clone.load(Ordering::Relaxed) {
      thread::sleep(Duration::from_secs(60));
      unsafe {
        let process = GetCurrentProcess();
        let _ = EmptyWorkingSet(process);
      }
    }
  });

  let (event_tx, event_rx) = channel::<EnforcerEvent>();

  // Enforcer Core states
  let mut input_enforcer = AudioEnforcer::new(
    AudioFlow::Input,
    input_target.clone(),
    event_tx.clone(),
    main_thread_id,
  )?;
  let mut output_enforcer = AudioEnforcer::new(
    AudioFlow::Output,
    output_target.clone(),
    event_tx.clone(),
    main_thread_id,
  )?;

  if !config.input_paused {
    input_enforcer.enable()?;
    input_enforcer.force_to_target();
  }

  if !config.output_paused {
    output_enforcer.enable()?;
    output_enforcer.force_to_target();
  }

  // Spawn config file monitor to update target volume dynamically if config changes
  let input_target_clone = input_target.clone();
  let output_target_clone = output_target.clone();
  let input_paused_clone = input_paused.clone();
  let output_paused_clone = output_paused.clone();
  let event_tx_clone = event_tx.clone();
  thread::spawn(move || {
    let mut last_modified = None;
    let mut last_input_target = input_target_clone.load(Ordering::SeqCst);
    let mut last_output_target = output_target_clone.load(Ordering::SeqCst);
    let mut last_input_paused = input_paused_clone.load(Ordering::SeqCst);
    let mut last_output_paused = output_paused_clone.load(Ordering::SeqCst);
    let config_path = Config::get_path().ok();

    loop {
      thread::sleep(Duration::from_millis(1500));
      if let Some(ref path) = config_path {
        let current_modified =
          fs::metadata(path).and_then(|m| m.modified()).ok();

        if current_modified.is_none() || current_modified != last_modified {
          last_modified = current_modified;
          if let Ok(cfg) = Config::load() {
            if cfg.input_target != last_input_target
              || cfg.output_target != last_output_target
              || cfg.input_paused != last_input_paused
              || cfg.output_paused != last_output_paused
            {
              last_input_target = cfg.input_target;
              last_output_target = cfg.output_target;
              last_input_paused = cfg.input_paused;
              last_output_paused = cfg.output_paused;

              input_target_clone.store(cfg.input_target, Ordering::SeqCst);
              output_target_clone.store(cfg.output_target, Ordering::SeqCst);
              input_paused_clone.store(cfg.input_paused, Ordering::SeqCst);
              output_paused_clone.store(cfg.output_paused, Ordering::SeqCst);

              let _ = event_tx_clone.send(EnforcerEvent::VolumeFileChanged);
              unsafe {
                let _ = PostThreadMessageW(
                  main_thread_id,
                  WM_WAKEUP,
                  WPARAM(0),
                  LPARAM(0),
                );
              }
            }
          }
        }
      }
    }
  });

  let mut msg = MSG::default();
  let mut exit_loop = false;
  while !exit_loop {
    // 1. Process all pending queue events (volume watcher updates, default device modifications)
    while let Ok(event) = event_rx.try_recv() {
      match event {
        EnforcerEvent::RebindRole(flow, role) => match flow {
          AudioFlow::Input => {
            if !input_paused.load(Ordering::SeqCst) {
              let _ = input_enforcer.bind_role(role);
              input_enforcer.force_to_target();
            }
          }
          AudioFlow::Output => {
            if !output_paused.load(Ordering::SeqCst) {
              let _ = output_enforcer.bind_role(role);
              output_enforcer.force_to_target();
            }
          }
        },
        EnforcerEvent::VolumeFileChanged => {
          let in_paused = input_paused.load(Ordering::SeqCst);
          let out_paused = output_paused.load(Ordering::SeqCst);

          if in_paused {
            let _ = input_enforcer.disable();
          } else {
            let _ = input_enforcer.enable();
            input_enforcer.force_to_target();
          }

          if out_paused {
            let _ = output_enforcer.disable();
          } else {
            let _ = output_enforcer.enable();
            output_enforcer.force_to_target();
          }

          tray_app.update_toggle_input_text(in_paused);
          tray_app.update_toggle_output_text(out_paused);
          let _ = tray_app.update_icon(in_paused, out_paused);
          tray_app.update_tooltip(
            input_target.load(Ordering::SeqCst),
            in_paused,
            output_target.load(Ordering::SeqCst),
            out_paused,
          );
        }
      }
    }

    // 2. Process pending tray interaction events
    while let Some(action) = tray_app.handle_events() {
      match action {
        TrayAction::ToggleInput => {
          let currently_paused = input_paused.load(Ordering::SeqCst);
          let next_paused = !currently_paused;
          input_paused.store(next_paused, Ordering::SeqCst);
          if next_paused {
            let _ = input_enforcer.disable();
          } else {
            let _ = input_enforcer.enable();
            input_enforcer.force_to_target();
          }
          tray_app.update_toggle_input_text(next_paused);
          let _ = tray_app
            .update_icon(next_paused, output_paused.load(Ordering::SeqCst));
          tray_app.update_tooltip(
            input_target.load(Ordering::SeqCst),
            next_paused,
            output_target.load(Ordering::SeqCst),
            output_paused.load(Ordering::SeqCst),
          );
          if let Ok(mut cfg) = Config::load() {
            cfg.input_paused = next_paused;
            let _ = cfg.save();
          }
        }
        TrayAction::ToggleOutput => {
          let currently_paused = output_paused.load(Ordering::SeqCst);
          let next_paused = !currently_paused;
          output_paused.store(next_paused, Ordering::SeqCst);
          if next_paused {
            let _ = output_enforcer.disable();
          } else {
            let _ = output_enforcer.enable();
            output_enforcer.force_to_target();
          }
          tray_app.update_toggle_output_text(next_paused);
          let _ = tray_app
            .update_icon(input_paused.load(Ordering::SeqCst), next_paused);
          tray_app.update_tooltip(
            input_target.load(Ordering::SeqCst),
            input_paused.load(Ordering::SeqCst),
            output_target.load(Ordering::SeqCst),
            next_paused,
          );
          if let Ok(mut cfg) = Config::load() {
            cfg.output_paused = next_paused;
            let _ = cfg.save();
          }
        }
        TrayAction::PromptSetTarget => {
          if let Ok(path) = Config::get_path() {
            if let Some(parent) = path.parent() {
              let _ = fs::create_dir_all(parent);
            }
            if !path.exists() {
              let _ = fs::write(&path, "input_target=100\noutput_target=100\ninput_paused=false\noutput_paused=false\n");
            }
            let _ = Command::new("notepad.exe").arg(&path).spawn();
          }
        }
        TrayAction::ToggleAutorun => {
          if is_autorun_registered() {
            let _ = deregister_autorun();
          } else {
            let _ = register_autorun();
          }
          tray_app.refresh_autorun_menu();
        }
        TrayAction::Exit => {
          exit_loop = true;
        }
      }
    }

    if exit_loop {
      break;
    }

    // 3. Block until a message is received
    unsafe {
      if GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if msg.message == WM_QUIT {
          break;
        }
        let _ = DispatchMessageW(&msg);
      }
    }
  }

  trimmer_running.store(false, Ordering::Relaxed);
  input_enforcer.disable()?;
  output_enforcer.disable()?;
  unsafe {
    CoUninitialize();
  }

  Ok(())
}
