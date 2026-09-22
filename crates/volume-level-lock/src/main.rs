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
    atomic::{AtomicBool, Ordering},
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
use enforcer::{AudioEnforcer, AudioFlow, EnforcerEvent, VolumeState};
#[cfg(windows)]
use instance::acquire_single_instance_guard;
#[cfg(windows)]
use registry::{deregister_autorun, is_autorun_registered, register_autorun};
#[cfg(windows)]
use tray::{TrayAction, TrayApp};
#[cfg(windows)]
use utils::wake_main_thread;
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
  let mut dirty = false;

  if let Some(target) = args.level {
    let clamped = target.clamp(1, 100);
    config.input_target = clamped;
    config.output_target = clamped;
    dirty = true;
  }

  if let Some(target) = args.input_level {
    config.input_target = target.clamp(1, 100);
    dirty = true;
  }

  if let Some(target) = args.output_level {
    config.output_target = target.clamp(1, 100);
    dirty = true;
  }

  if dirty {
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
fn apply_enforcer_state(enforcer: &mut AudioEnforcer, paused: bool) {
  if paused {
    let _ = enforcer.disable();
  } else {
    let _ = enforcer.enable();
    enforcer.force_to_target();
  }
}

#[cfg(windows)]
fn sync_tray_ui(tray_app: &TrayApp, state: &VolumeState) {
  let input_target = state.input_target.load(Ordering::SeqCst);
  let input_paused = state.input_paused.load(Ordering::SeqCst);
  let output_target = state.output_target.load(Ordering::SeqCst);
  let output_paused = state.output_paused.load(Ordering::SeqCst);

  tray_app.update_toggle_input_text(input_paused);
  tray_app.update_toggle_output_text(output_paused);
  let _ = tray_app.update_icon(input_paused, output_paused);
  tray_app.update_tooltip(
    input_target,
    input_paused,
    output_target,
    output_paused,
  );
}

#[cfg(windows)]
fn toggle_and_sync(
  enforcer: &mut AudioEnforcer,
  flow: AudioFlow,
  state: &VolumeState,
  tray_app: &TrayApp,
) {
  let paused_atomic = state.paused(flow);
  let next_paused = !paused_atomic.load(Ordering::SeqCst);
  paused_atomic.store(next_paused, Ordering::SeqCst);
  apply_enforcer_state(enforcer, next_paused);

  sync_tray_ui(tray_app, state);

  let _ = Config::from_state(state).save();
}

#[cfg(windows)]
fn run_enforcer(config: Config) -> Result<()> {
  unsafe {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
  }

  let state = Arc::new(VolumeState::new(
    config.input_target,
    config.output_target,
    config.input_paused,
    config.output_paused,
  ));
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
    state.clone(),
    event_tx.clone(),
    main_thread_id,
  )?;
  let mut output_enforcer = AudioEnforcer::new(
    AudioFlow::Output,
    state.clone(),
    event_tx.clone(),
    main_thread_id,
  )?;

  apply_enforcer_state(&mut input_enforcer, config.input_paused);
  apply_enforcer_state(&mut output_enforcer, config.output_paused);

  // Spawn config file monitor to update target volume dynamically if config changes
  let watcher_state = state.clone();
  let event_tx_clone = event_tx.clone();
  thread::spawn(move || {
    let mut last_modified = None;
    let config_path = Config::get_path().ok();

    loop {
      thread::sleep(Duration::from_millis(1500));

      let Some(ref path) = config_path else {
        continue;
      };

      let current_modified = fs::metadata(path).and_then(|m| m.modified()).ok();

      // Nothing changed on disk, skip the reparse entirely.
      if current_modified.is_some() && current_modified == last_modified {
        continue;
      }
      last_modified = current_modified;

      let Ok(cfg) = Config::load() else {
        continue;
      };

      // Compare directly against the live atomics rather than a thread-local shadow copy
      // that can drift out of sync with tray-driven toggles.
      let changed = cfg.input_target
        != watcher_state.input_target.load(Ordering::SeqCst)
        || cfg.output_target
          != watcher_state.output_target.load(Ordering::SeqCst)
        || cfg.input_paused
          != watcher_state.input_paused.load(Ordering::SeqCst)
        || cfg.output_paused
          != watcher_state.output_paused.load(Ordering::SeqCst);

      if !changed {
        continue;
      }

      watcher_state
        .input_target
        .store(cfg.input_target, Ordering::SeqCst);
      watcher_state
        .output_target
        .store(cfg.output_target, Ordering::SeqCst);
      watcher_state
        .input_paused
        .store(cfg.input_paused, Ordering::SeqCst);
      watcher_state
        .output_paused
        .store(cfg.output_paused, Ordering::SeqCst);

      let _ = event_tx_clone.send(EnforcerEvent::VolumeFileChanged);
      wake_main_thread(main_thread_id);
    }
  });

  let mut msg = MSG::default();
  let mut exit_loop = false;
  while !exit_loop {
    // 1. Process all pending queue events (volume watcher updates, default device modifications)
    while let Ok(event) = event_rx.try_recv() {
      match event {
        EnforcerEvent::RebindRole(flow, role) => {
          let enforcer = match flow {
            AudioFlow::Input => &mut input_enforcer,
            AudioFlow::Output => &mut output_enforcer,
          };
          if !state.paused(flow).load(Ordering::SeqCst) {
            let _ = enforcer.bind_role(role);
            enforcer.force_to_target();
          }
        }
        EnforcerEvent::VolumeFileChanged => {
          let in_paused = state.input_paused.load(Ordering::SeqCst);
          let out_paused = state.output_paused.load(Ordering::SeqCst);

          apply_enforcer_state(&mut input_enforcer, in_paused);
          apply_enforcer_state(&mut output_enforcer, out_paused);

          sync_tray_ui(&tray_app, &state);
        }
      }
    }

    // 2. Process pending tray interaction events
    while let Some(action) = tray_app.handle_events() {
      match action {
        TrayAction::ToggleInput => {
          toggle_and_sync(
            &mut input_enforcer,
            AudioFlow::Input,
            &state,
            &tray_app,
          );
        }
        TrayAction::ToggleOutput => {
          toggle_and_sync(
            &mut output_enforcer,
            AudioFlow::Output,
            &state,
            &tray_app,
          );
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
