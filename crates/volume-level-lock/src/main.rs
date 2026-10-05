#![cfg_attr(not(test), windows_subsystem = "windows")]

mod config;
mod enforcer;
mod instance;
mod registry;
mod tray;
mod utils;

#[cfg(windows)]
use std::{
  fs,
  process::Command,
  sync::{
    atomic::Ordering,
    mpsc::{channel, Sender},
    Arc,
  },
  thread,
  time::Duration,
};

use anyhow::Result;
#[cfg(windows)]
use clap::Parser;
#[cfg(windows)]
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
    DispatchMessageW, GetMessageW, PostThreadMessageW, MSG, WM_QUIT,
  },
};

#[cfg(windows)]
const TRIMMER_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(windows)]
const CONFIG_POLL_INTERVAL: Duration = Duration::from_millis(1500);

#[cfg(windows)]
#[derive(Parser)]
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
}

fn main() -> Result<()> {
  #[cfg(not(windows))]
  {
    println!("This application is only supported on Windows.");
    return Ok(());
  }

  #[cfg(windows)]
  run_windows(Args::parse())
}

#[cfg(windows)]
fn run_windows(args: Args) -> Result<()> {
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

  // Autorun changes are one-shot admin commands, so they run whether or
  // not the tray app already holds the single-instance guard.
  if args.install {
    // Start the app so a fresh install is running right away. When an
    // instance is already up, the single-instance check ends this copy
    // immediately, so the spawn is best effort.
    let _ = Command::new(register_autorun()?).spawn();
    return Ok(());
  }

  if args.uninstall {
    deregister_autorun();
    return Ok(());
  }

  // Quietly exit if another instance already holds the guard.
  let Some(_guard) = acquire_single_instance_guard()? else {
    return Ok(());
  };

  run_enforcer(config)
}

#[cfg(windows)]
fn apply_paused_state(enforcer: &mut AudioEnforcer, paused: bool) {
  if paused {
    enforcer.disable();
  } else {
    let _ = enforcer.enable();
    enforcer.force_to_target();
  }
}

#[cfg(windows)]
fn toggle_and_sync(
  enforcer: &mut AudioEnforcer,
  state: &VolumeState,
  tray_app: &TrayApp,
) {
  let paused_atomic = state.paused(enforcer.flow());
  let next_paused = !paused_atomic.load(Ordering::SeqCst);
  paused_atomic.store(next_paused, Ordering::SeqCst);
  apply_paused_state(enforcer, next_paused);

  tray_app.refresh(state);
  let _ = Config::from_state(state).save();
}

#[cfg(windows)]
fn run_enforcer(config: Config) -> Result<()> {
  // SAFETY: this runs before any other COM use on the main thread, and
  // `CoUninitialize` runs on every exit path below.
  unsafe {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
  }

  let state = Arc::new(VolumeState::new(
    config.input_target,
    config.output_target,
    config.input_paused,
    config.output_paused,
  ));
  // SAFETY: the calling thread is the one that owns the message queue
  // and the tray, and the id stays valid for the life of the process.
  let main_thread_id = unsafe { GetCurrentThreadId() };

  let tray_app = TrayApp::new(&state, main_thread_id)?;

  // Exit cleanly on CTRL-C by waking the message loop with WM_QUIT.
  // SAFETY: the message queue belongs to this thread and is drained by
  // the loop at the end of this function, so the id stays valid.
  ctrlc::set_handler(move || unsafe {
    let _ = PostThreadMessageW(main_thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
  })?;

  // Detached: this thread ends with the process, so it needs no shutdown
  // signal. The sleeping happens before the Win32 call, so the handle
  // below never outlives the process either.
  thread::spawn(|| loop {
    thread::sleep(TRIMMER_INTERVAL);
    // SAFETY: `GetCurrentProcess` returns a process-wide pseudo-handle
    // that is always valid, and `EmptyWorkingSet` only trims pages, so
    // it cannot fail in a way that matters here.
    unsafe {
      let _ = EmptyWorkingSet(GetCurrentProcess());
    }
  });

  let (event_tx, event_rx) = channel::<EnforcerEvent>();

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

  apply_paused_state(&mut input_enforcer, config.input_paused);
  apply_paused_state(&mut output_enforcer, config.output_paused);

  spawn_config_watcher(state.clone(), event_tx, main_thread_id);

  let mut msg = MSG::default();
  'main: loop {
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
        EnforcerEvent::ConfigChanged => {
          apply_paused_state(
            &mut input_enforcer,
            state.input_paused.load(Ordering::SeqCst),
          );
          apply_paused_state(
            &mut output_enforcer,
            state.output_paused.load(Ordering::SeqCst),
          );
          tray_app.refresh(&state);
        }
      }
    }

    // 2. Process pending tray interaction events
    while let Some(action) = tray_app.handle_events() {
      match action {
        TrayAction::ToggleInput => {
          toggle_and_sync(&mut input_enforcer, &state, &tray_app);
        }
        TrayAction::ToggleOutput => {
          toggle_and_sync(&mut output_enforcer, &state, &tray_app);
        }
        TrayAction::PromptSetTarget => {
          if let Ok(path) = Config::get_path() {
            // Seed a file for the editor only when none exists, so
            // opening settings never overwrites what is already there.
            if !path.exists() {
              let _ = Config::default().save_to_path(&path);
            }
            let _ = Command::new("notepad.exe").arg(&path).spawn();
          }
        }
        TrayAction::ToggleAutorun => {
          if is_autorun_registered() {
            deregister_autorun();
          } else {
            let _ = register_autorun();
          }
          tray_app.refresh_autorun_menu();
        }
        TrayAction::Exit => break 'main,
      }
    }

    // 3. Block until a message is received
    // SAFETY: `msg` is a live `MSG` buffer for the whole call, and
    // `None` means the thread's own queue, which this thread drains.
    let received = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
    // 0 means WM_QUIT was retrieved, which is not written into `msg`, and
    // -1 means the call failed. Neither is a message to dispatch, and
    // looping on either would spin instead of exit.
    if received <= 0 {
      break;
    }

    // SAFETY: `received > 0` means `msg` holds a message this thread's
    // queue produced, so it is valid to dispatch.
    unsafe {
      let _ = DispatchMessageW(&msg);
    }
  }

  // Disabled explicitly rather than left to `Drop`, because
  // unregistering the endpoint callbacks is a COM call and those drops
  // would otherwise run after `CoUninitialize` below.
  input_enforcer.disable();
  output_enforcer.disable();
  // SAFETY: balances the `CoInitializeEx` at the top of this function.
  unsafe {
    CoUninitialize();
  }

  Ok(())
}

#[cfg(windows)]
fn spawn_config_watcher(
  state: Arc<VolumeState>,
  event_tx: Sender<EnforcerEvent>,
  main_thread_id: u32,
) {
  thread::spawn(move || {
    let Ok(config_path) = Config::get_path() else {
      return;
    };
    let mut last_modified = None;

    loop {
      thread::sleep(CONFIG_POLL_INTERVAL);

      let current_modified =
        fs::metadata(&config_path).and_then(|m| m.modified()).ok();

      // Nothing changed on disk, skip the reparse entirely. A file that
      // does not exist yet compares equal to a missing one, so the first
      // write after startup is still picked up.
      if current_modified == last_modified {
        continue;
      }
      last_modified = current_modified;

      let Ok(config) = Config::load_from_path(&config_path) else {
        continue;
      };
      if !config.apply_to(&state) {
        continue;
      }

      let _ = event_tx.send(EnforcerEvent::ConfigChanged);
      wake_main_thread(main_thread_id);
    }
  });
}
