#![cfg_attr(not(test), windows_subsystem = "windows")]

mod config;
mod enforcer;
mod instance;
mod registry;
mod tray;

use std::{
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
use enforcer::{AudioEnforcer, EnforcerEvent};
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
  Win32::System::Threading::GetCurrentProcess,
  Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PostThreadMessageW, MSG, WM_QUIT,
  },
};

/// Lock default volume at a fixed target level.
#[derive(Parser, Debug)]
#[command(name = "vol-level-lock", about = "Locks volume level")]
struct Args {
  /// Level to lock the volume at (1-100)
  #[arg(short, long)]
  level: Option<u32>,

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
    config.target_percent = target.clamp(1, 100);
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

  let target_percent = Arc::new(AtomicU32::new(config.target_percent));
  let is_paused = Arc::new(AtomicBool::new(false));
  let main_thread_id =
    unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };

  // Setup System Tray icon
  let tray_app = TrayApp::new(config.target_percent, false)?;

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

  // Enforcer Core state
  let target_percent_clone = target_percent.clone();
  let mut enforcer =
    AudioEnforcer::new(target_percent_clone, event_tx.clone())?;
  enforcer.enable()?;
  enforcer.force_to_target();

  // Spawn config file monitor to update target volume dynamically if config.txt changes
  let target_percent_clone2 = target_percent.clone();
  let event_tx_clone = event_tx.clone();
  thread::spawn(move || {
    let mut last_percent = target_percent_clone2.load(Ordering::SeqCst);
    loop {
      thread::sleep(Duration::from_millis(1500));
      if let Ok(cfg) = Config::load() {
        if cfg.target_percent != last_percent {
          last_percent = cfg.target_percent;
          target_percent_clone2.store(last_percent, Ordering::SeqCst);
          let _ = event_tx_clone.send(EnforcerEvent::VolumeFileChanged);
        }
      }
    }
  });

  unsafe {
    let mut msg = MSG::default();
    let mut exit_loop = false;
    while !exit_loop {
      // 1. Process all pending queue events (volume watcher updates, default device modifications)
      while let Ok(event) = event_rx.try_recv() {
        match event {
          EnforcerEvent::RebindRole(role) => {
            if !is_paused.load(Ordering::SeqCst) {
              let _ = enforcer.bind_role(role);
              enforcer.force_to_target();
            }
          }
          EnforcerEvent::VolumeFileChanged => {
            if !is_paused.load(Ordering::SeqCst) {
              enforcer.force_to_target();
            }
            tray_app.update_tooltip(target_percent.load(Ordering::SeqCst));
          }
        }
      }

      // 2. Process pending tray interaction events
      while let Some(action) = tray_app.handle_events() {
        match action {
          TrayAction::ToggleEnforcement => {
            let currently_paused = is_paused.load(Ordering::SeqCst);
            let next_paused = !currently_paused;
            is_paused.store(next_paused, Ordering::SeqCst);
            if next_paused {
              let _ = enforcer.disable();
            } else {
              let _ = enforcer.enable();
              enforcer.force_to_target();
            }
            tray_app.update_toggle_text(next_paused);
            let _ = tray_app.update_icon(next_paused);
          }
          TrayAction::PromptSetTarget => {
            if let Ok(path) = Config::get_path() {
              if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
              }
              if !path.exists() {
                let _ = std::fs::write(&path, "100");
              }
              let _ =
                std::process::Command::new("notepad.exe").arg(&path).spawn();
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

      // 3. Process Windows window messages in a non-blocking check
      while windows::Win32::UI::WindowsAndMessaging::PeekMessageW(
        &mut msg,
        None,
        0,
        0,
        windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
      )
      .as_bool()
      {
        if msg.message == WM_QUIT {
          exit_loop = true;
          break;
        }
        let _ = DispatchMessageW(&msg);
      }

      thread::sleep(Duration::from_millis(10));
    }
  }

  trimmer_running.store(false, Ordering::Relaxed);
  enforcer.disable()?;
  unsafe {
    CoUninitialize();
  }

  Ok(())
}
