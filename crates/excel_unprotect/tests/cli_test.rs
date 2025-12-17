use std::{
  env, fs,
  path::{Path, PathBuf},
};

use assert_cmd::Command;
use fs::File;
use predicates::prelude::{predicate::str as pstr, *};
use tempfile::TempDir;
use zip::ZipArchive;

const CARGO_BIN_EXE: &str = env!("CARGO_BIN_EXE_excel_unprotect");

fn get_fixtures_path() -> PathBuf {
  let root = env::current_dir().unwrap();
  if root.ends_with("crates/excel_unprotect") {
    root.join("tests/fixtures")
  } else {
    root.join("crates/excel_unprotect/tests/fixtures")
  }
}

fn verify_zip(file_path: &Path) {
  let file =
    File::open(file_path).expect("Failed to open file for verification");
  if let Err(e) = ZipArchive::new(file) {
    panic!(
      "File {} is not a valid zip archive: {}",
      file_path.display(),
      e
    );
  }
}

#[test]
fn test_plain_file() {
  let fixtures = get_fixtures_path();
  let temp_dir = TempDir::new().unwrap();
  let file_path = fixtures.join("plain.xlsx");

  let temp_file_path = temp_dir.path().join("plain.xlsx");
  fs::copy(&file_path, &temp_file_path).unwrap();

  let clean_path = temp_dir.path().join("plain_clean.xlsx");

  assert!(
    temp_file_path.exists(),
    "Fixture copy not found: {}",
    temp_file_path.display()
  );

  let mut cmd = Command::new(CARGO_BIN_EXE);
  cmd
    .arg(&temp_file_path)
    .assert()
    .success()
    .stdout(pstr::contains("Processing file:"));

  assert!(
    clean_path.exists(),
    "Clean file not created: {}",
    clean_path.display()
  );

  verify_zip(&clean_path);
}

#[test]
fn test_protected() {
  let fixtures = get_fixtures_path();
  let temp_dir = TempDir::new().unwrap();
  let file_path = fixtures.join("protected.xlsx");

  let temp_file_path = temp_dir.path().join("protected.xlsx");
  fs::copy(&file_path, &temp_file_path).unwrap();

  let clean_path = temp_dir.path().join("protected_clean.xlsx");

  assert!(
    temp_file_path.exists(),
    "Fixture copy not found: {}",
    temp_file_path.display()
  );

  let mut cmd = Command::new(CARGO_BIN_EXE);
  cmd
    .arg(&temp_file_path)
    .assert()
    .success()
    .stdout(pstr::contains("Sheet protection removed"));

  assert!(
    clean_path.exists(),
    "Clean file not created: {}",
    clean_path.display()
  );

  verify_zip(&clean_path);
}

#[test]
fn test_encrypted_with_password() {
  let fixtures = get_fixtures_path();
  let temp_dir = TempDir::new().unwrap();
  let file_path = fixtures.join("encrypted.xlsx");

  if !file_path.exists() {
    println!("Skipping encrypted test as fixture is missing");
    return;
  }

  let temp_file_path = temp_dir.path().join("encrypted.xlsx");
  fs::copy(&file_path, &temp_file_path).unwrap();

  let decrypted_path = temp_dir.path().join("encrypted_decrypted.xlsx");
  let clean_path = temp_dir.path().join("encrypted_clean.xlsx");

  let mut cmd = Command::new(CARGO_BIN_EXE);
  let assert = cmd
    .arg(&temp_file_path)
    .arg("--pass")
    .arg("password")
    .assert();

  let output = assert.get_output();
  let stdout = std::str::from_utf8(&output.stdout).unwrap();
  println!("Stdout: {}", stdout);

  if !clean_path.exists() {
    panic!("Clean file not created. Stdout: {}", stdout);
  }

  assert
    .success()
    .stdout(pstr::contains("File decrypted successfully"));

  assert!(
    clean_path.exists(),
    "Clean file not created: {}",
    clean_path.display()
  );
  assert!(
    decrypted_path.exists(),
    "Decrypted file not found: {}",
    decrypted_path.display()
  );

  verify_zip(&clean_path);
  verify_zip(&decrypted_path);
}

#[test]
fn test_encrypted_wrong_password() {
  let fixtures = get_fixtures_path();
  let temp_dir = TempDir::new().unwrap();
  let file_path = fixtures.join("encrypted.xlsx");

  if !file_path.exists() {
    return;
  }

  let temp_file_path = temp_dir.path().join("encrypted.xlsx");
  fs::copy(&file_path, &temp_file_path).unwrap();

  let mut cmd = Command::new(CARGO_BIN_EXE);
  let assert = cmd
    .arg(&temp_file_path)
    .arg("--pass")
    .arg("wrongpass")
    .assert();

  assert.success().stdout(
    pstr::contains("Failed to process")
      .or(pstr::contains("Provided password failed")),
  );
}

#[test]
fn test_corrupt_file() {
  let fixtures = get_fixtures_path();
  let temp_dir = TempDir::new().unwrap();
  let file_path = fixtures.join("corrupt.xlsx");

  let temp_file_path = temp_dir.path().join("corrupt.xlsx");
  fs::copy(&file_path, &temp_file_path).unwrap();

  assert!(
    temp_file_path.exists(),
    "Fixture copy not found: {}",
    temp_file_path.display()
  );

  let mut cmd = Command::new(CARGO_BIN_EXE);
  cmd
    .arg(&temp_file_path)
    .assert()
    .success()
    .stdout(pstr::contains("Failed to open as Zip archive"));
}

#[test]
fn test_non_existent_file() {
  let file_path = "non_existent.xlsx";

  let mut cmd = Command::new(CARGO_BIN_EXE);
  cmd
    .arg(file_path)
    .assert()
    .success()
    .stdout(pstr::contains("File not found"));
}
