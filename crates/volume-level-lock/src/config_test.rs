use tempfile::tempdir;

use super::*;

#[test]
fn test_load_non_existent() {
  let temp_dir = tempdir().unwrap();
  let file_path = temp_dir.path().join("config.ini");
  let cfg = Config::load_from_path(&file_path).unwrap();
  assert_eq!(cfg.input_target, 100);
  assert_eq!(cfg.output_target, 100);
  assert!(!cfg.input_paused);
  assert!(!cfg.output_paused);
}

#[test]
fn test_load_key_value() {
  let temp_dir = tempdir().unwrap();
  let file_path = temp_dir.path().join("config.ini");
  fs::write(
    &file_path,
    "input_target=45\noutput_target=90\ninput_paused=true\noutput_paused=false\n",
  )
  .unwrap();
  let cfg = Config::load_from_path(&file_path).unwrap();
  assert_eq!(cfg.input_target, 45);
  assert_eq!(cfg.output_target, 90);
  assert!(cfg.input_paused);
  assert!(!cfg.output_paused);
}

#[test]
fn test_save_and_load() {
  let temp_dir = tempdir().unwrap();
  let file_path = temp_dir.path().join("config.ini");
  let cfg = Config {
    input_target: 30,
    output_target: 80,
    input_paused: false,
    output_paused: true,
  };
  cfg.save_to_path(&file_path).unwrap();
  let loaded = Config::load_from_path(&file_path).unwrap();
  assert_eq!(loaded.input_target, 30);
  assert_eq!(loaded.output_target, 80);
  assert!(!loaded.input_paused);
  assert!(loaded.output_paused);
}

#[test]
fn test_corrupted_config_rewritten() {
  let temp_dir = tempdir().unwrap();
  let file_path = temp_dir.path().join("config.ini");
  fs::write(&file_path, "invalid_junk_data_here").unwrap();

  // Loading should detect corruption, delete the file, write defaults, and return default
  let cfg = Config::load_from_path(&file_path).unwrap();
  assert_eq!(cfg.input_target, 100);
  assert_eq!(cfg.output_target, 100);
  assert!(!cfg.input_paused);
  assert!(!cfg.output_paused);

  // Verify the file was indeed replaced with defaults
  let replaced_content = fs::read_to_string(&file_path).unwrap();
  assert!(replaced_content.contains("input_target=100"));
  assert!(replaced_content.contains("output_target=100"));
  assert!(replaced_content.contains("input_paused=false"));
  assert!(replaced_content.contains("output_paused=false"));
}

#[test]
fn test_partial_or_unknown_key_resets_every_field() {
  // A file is all-or-nothing: one missing field or one unknown key
  // discards the valid fields alongside the bad one.
  for content in [
    "input_target=45\noutput_target=90\ninput_paused=true\n",
    "input_target=45\noutput_target=90\ninput_paused=true\noutput_paused=false\nmystery=1\n",
  ] {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("config.ini");
    fs::write(&file_path, content).unwrap();

    let cfg = Config::load_from_path(&file_path).unwrap();
    assert_eq!(cfg, Config::default(), "for content: {content}");
  }
}

#[test]
fn test_from_state_snapshot() {
  let state = VolumeState::new(42, 77, true, false);
  let cfg = Config::from_state(&state);
  assert_eq!(cfg.input_target, 42);
  assert_eq!(cfg.output_target, 77);
  assert!(cfg.input_paused);
  assert!(!cfg.output_paused);
}

#[test]
fn test_apply_to_reports_and_stores_changes() {
  let state = VolumeState::new(42, 77, true, false);
  let unchanged = Config::from_state(&state);
  assert!(!unchanged.apply_to(&state));

  let changed = Config {
    input_paused: false,
    output_target: 10,
    ..unchanged
  };
  assert!(changed.apply_to(&state));

  // A second pass must see the state it just wrote and report no
  // change, otherwise the file watcher would reload forever.
  assert!(!changed.apply_to(&state));
  assert_eq!(Config::from_state(&state), changed);
}
