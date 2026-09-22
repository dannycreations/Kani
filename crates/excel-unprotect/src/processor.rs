use std::{
  fs::File,
  io::{stdin, stdout, BufReader, Cursor, Read, Seek, SeekFrom, Write},
  path::Path,
};

use anyhow::{bail, Context, Result};

use crate::{
  cleaner::remove_protection_and_save,
  crypto::{decrypt_file, is_ole_file},
};

pub fn process_file(
  file_path: &Path,
  password: Option<&str>,
  disable_macros: bool,
) -> Result<()> {
  let mut file = File::open(file_path).with_context(|| {
    println!("[!] File not found: {}", file_path.display());
    format!("Failed to open input file: {}", file_path.display())
  })?;

  let mut header = [0u8; 8];
  let n = file.read(&mut header)?;
  file.seek(SeekFrom::Start(0))?;

  let is_encrypted = n >= 8 && is_ole_file(&header);

  let decrypted = if is_encrypted {
    println!("[!] File is encrypted with a password-to-open.");
    Some(match password {
      Some(pass) => attempt_decryption(&mut file, pass)?,
      None => find_valid_password(&mut file)?,
    })
  } else {
    None
  };

  println!("[+] Processing file: {}", file_path.display());

  let result = match decrypted {
    Some(bytes) => {
      remove_protection_and_save(Cursor::new(bytes), file_path, disable_macros)
    }
    None => remove_protection_and_save(
      BufReader::new(file),
      file_path,
      disable_macros,
    ),
  };

  if let Err(e) = result {
    println!("[!] Failed to process {}: {}", file_path.display(), e);
    return Err(e);
  }

  Ok(())
}

fn find_valid_password(file: &mut File) -> Result<Vec<u8>> {
  const MAX_ATTEMPTS: u32 = 3;
  for attempt in 1..=MAX_ATTEMPTS {
    print!("Enter file password (attempt {attempt}/{MAX_ATTEMPTS}): ");
    stdout().flush()?;

    let mut input_pass = String::new();
    stdin().read_line(&mut input_pass)?;
    let input_pass = input_pass.trim();

    if let Ok(data) = attempt_decryption(file, input_pass) {
      return Ok(data);
    }
  }

  bail!("Maximum attempts reached or decryption failed.")
}

fn attempt_decryption(file: &mut File, password: &str) -> Result<Vec<u8>> {
  file.seek(SeekFrom::Start(0))?;

  let capacity = file.metadata().map(|m| m.len() as usize).unwrap_or(0);
  let mut buffer = Vec::with_capacity(capacity);

  if let Err(e) = decrypt_file(&mut *file, &mut buffer, password) {
    println!("[!] Password failed: {e}");
    return Err(e);
  }

  println!("[+] File decrypted successfully.");
  Ok(buffer)
}
