use std::{
  fs::{self, File},
  io::{self, BufWriter, Read, Seek, SeekFrom, Write},
  path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};

use crate::{
  cleaner::remove_protection_and_save,
  crypto::{decrypt_file, is_ole_file},
  fs::add_suffix,
};

pub fn process_file(file_path: &Path, password: Option<&str>) -> Result<()> {
  if !file_path.exists() {
    println!("[!] File not found: {}", file_path.display());
    return Err(anyhow!("File not found"));
  }

  let decrypted_path = add_suffix(file_path, "_decrypted");

  match try_decrypt_and_save(file_path, &decrypted_path, password) {
    Ok(working_path) => {
      println!("[+] Processing file: {}", file_path.display());
      let result = remove_protection_and_save(&working_path, file_path);
      if let Err(ref e) = result {
        println!("[!] Failed to process {}: {}", file_path.display(), e);
      }
      if working_path == decrypted_path && working_path.exists() {
        let _ = fs::remove_file(&working_path);
      }
      result
    }
    Err(e) => {
      println!("[!] Decryption failed or skipped: {}", e);
      Err(e)
    }
  }
}

fn try_decrypt_and_save(
  input_path: &Path,
  output_path: &Path,
  password: Option<&str>,
) -> Result<PathBuf> {
  let mut file = File::open(input_path).context("Failed to open input file")?;

  let mut header = [0u8; 9];
  let n = file.read(&mut header)?;

  if n < 9 || !is_ole_file(&header[..n]) {
    return Ok(input_path.to_path_buf());
  }

  println!("[!] File is encrypted with a password-to-open.");

  if let Some(pass) = password {
    attempt_decryption(&mut file, output_path, pass)
  } else {
    find_valid_password(&mut file, output_path)
  }
}

fn find_valid_password(file: &mut File, output_path: &Path) -> Result<PathBuf> {
  let max_attempts = 3;
  for attempt in 1..=max_attempts {
    print!(
      "Enter file password (attempt {}/{}): ",
      attempt, max_attempts
    );
    io::stdout().flush()?;

    let mut input_pass = String::new();
    io::stdin().read_line(&mut input_pass)?;
    let input_pass = input_pass.trim();

    if let Ok(path) = attempt_decryption(file, output_path, input_pass) {
      return Ok(path);
    }
  }

  Err(anyhow!("Maximum attempts reached or decryption failed."))
}

fn attempt_decryption(
  file: &mut File,
  output_path: &Path,
  password: &str,
) -> Result<PathBuf> {
  file.seek(SeekFrom::Start(0))?;

  let out = File::create(output_path)?;
  let mut writer = BufWriter::new(out);

  let file_clone = file.try_clone()?;

  match decrypt_file(file_clone, &mut writer, password) {
    Ok(_) => {
      writer.flush()?;
      println!("[+] File decrypted successfully: {}", output_path.display());
      Ok(output_path.to_path_buf())
    }
    Err(e) => {
      drop(writer);
      let _ = fs::remove_file(output_path);
      println!("[!] Password failed: {}", e);
      Err(e)
    }
  }
}
