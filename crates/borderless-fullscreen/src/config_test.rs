use super::*;

fn config_with(process_names: &str) -> Config {
  Config::parse_ini(&format!("[settings]\nprocess_names = {process_names}\n"))
}

#[test]
fn matching_ignores_case_and_exe_suffix() {
  let config = config_with("Game.exe, notepad");

  for name in ["Game", "game", "GAME.EXE", "game.Exe", "Notepad"] {
    assert!(config.is_monitored(name), "{name} should be monitored");
  }
  for name in ["gam", "games", "notepad2", "ote"] {
    assert!(!config.is_monitored(name), "{name} should not match");
  }
}

#[test]
fn matching_handles_names_that_end_mid_character() {
  // The last four bytes of each of these names fall inside a multi-byte
  // character, so a suffix check cannot slice them as `&str`.
  let config = config_with("sp\u{20ac}le, \u{20ac}le.exe, \u{20ac}xe");

  for name in ["sp\u{20ac}le", "\u{20ac}le", "\u{20ac}xe"] {
    assert!(config.is_monitored(name), "{name} should be monitored");
  }
  assert!(!config.is_monitored("le"), "suffix fragment only");
}

#[test]
fn add_and_remove_keep_the_lookup_in_sync() {
  let mut config = config_with("game");
  assert!(!config.add_process("GAME"), "duplicate ignored");

  assert!(config.add_process("notes.exe"));
  assert_eq!(config.process_names, vec!["game", "notes"]);
  assert!(config.is_monitored("NOTES.exe"));

  assert!(config.remove_process("game.exe"));
  assert!(!config.is_monitored("Game"));
  assert!(!config.remove_process("game"), "already absent");
}

#[test]
fn load_or_create_seeds_a_missing_file_and_keeps_an_existing_one() {
  let path = std::env::temp_dir().join("borderless_fullscreen_cfg_test.ini");
  let _ = std::fs::remove_file(&path);

  let seeded = Config::load_or_create(&path);
  assert_eq!(seeded, Config::default());
  assert_eq!(
    std::fs::read_to_string(&path).unwrap(),
    seeded.serialize_ini()
  );

  let mut edited = seeded;
  edited.add_process("game.exe");
  edited.save(&path).unwrap();

  let reloaded = Config::load_or_create(&path);
  assert!(reloaded.is_monitored("game"), "reloaded keeps the process");
  assert_eq!(reloaded.polling_interval_ms, edited.polling_interval_ms);

  std::fs::remove_file(&path).unwrap();
}

#[test]
fn ini_round_trip_preserves_matching() {
  let mut config = config_with("Game.exe");
  config.polling_interval_ms = 5000;
  config.min_window_size = 640;

  let reparsed = Config::parse_ini(&config.serialize_ini());

  assert_eq!(reparsed.process_names, vec!["Game".to_owned()]);
  assert_eq!(reparsed.polling_interval_ms, 5000);
  assert_eq!(reparsed.min_window_size, 640);
  assert!(reparsed.is_monitored("game"));
}
