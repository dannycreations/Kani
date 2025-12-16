use std::{
  fs::File,
  io::{self, Read, Write},
  path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};

use crate::{
  cleaner::remove_protection_and_save,
  crypto::{decrypt_office_file, is_ole_file},
  fs::add_suffix,
};

pub fn process_file(file_path: &Path) {
  if !file_path.exists() {
    println!("[!] File not found: {}", file_path.display());
    return;
  }

  let decrypted_path = add_suffix(file_path, "_decrypted");

  match try_decrypt_and_save(file_path, &decrypted_path) {
    Ok(working_path) => {
      println!("[+] Processing file: {}", file_path.display());
      if let Err(e) = remove_protection_and_save(&working_path, file_path) {
        println!("[!] Failed to process {}: {}", file_path.display(), e);
      }

      if working_path != file_path && working_path.exists() {
        // std::fs::remove_file(&working_path).ok();
      }
    }
    Err(e) => {
      println!("[!] Decryption failed or skipped: {}", e);
    }
  }
}

fn try_decrypt_and_save(
  input_path: &Path,
  output_path: &Path,
) -> Result<PathBuf> {
  let mut file = File::open(input_path).context("Failed to open input file")?;
  let mut buffer = Vec::new();
  file.read_to_end(&mut buffer)?;

  if !is_ole_file(&buffer) {
    return Ok(input_path.to_path_buf());
  }

  println!("[!] File is encrypted with a password-to-open.");
  let max_attempts = 3;

  for attempt in 1..=max_attempts {
    print!(
      "Enter file password (attempt {}/{}): ",
      attempt, max_attempts
    );
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    match decrypt_office_file(&buffer, &password) {
      Ok(decrypted_data) => {
        let mut out = File::create(output_path)?;
        out.write_all(&decrypted_data)?;
        println!("[+] File decrypted successfully: {}", output_path.display());
        return Ok(output_path.to_path_buf());
      }
      Err(e) => {
        println!("[!] Wrong password or decryption failed: {}", e);
      }
    }
  }

  Err(anyhow!("Maximum attempts reached or decryption failed."))
}
