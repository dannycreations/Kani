use std::{
  fs::File,
  io::{stdin, stdout, BufWriter, Read, Seek, SeekFrom, Write},
  path::Path,
};

use anyhow::{anyhow, bail, Context, Result};
use tempfile::NamedTempFile;
use zip::ZipArchive;

use crate::{
  cleaner::remove_protection_and_save,
  crypto::{decrypt_file, is_ole_file},
};

pub fn process_file(
  file_path: &Path,
  password: Option<&str>,
  disable_macros: bool,
) -> Result<()> {
  if !file_path.exists() {
    println!("[!] File not found: {}", file_path.display());
    bail!("File not found");
  }

  let mut header = [0u8; 8];
  let mut file = File::open(file_path).context("Failed to open input file")?;

  let n = file.read(&mut header)?;
  file.seek(SeekFrom::Start(0))?;

  let is_encrypted = n >= 8 && is_ole_file(&header);

  let temp_file = if is_encrypted {
    println!("[!] File is encrypted with a password-to-open.");
    Some(match password {
      Some(pass) => attempt_decryption(&mut file, pass)?,
      None => find_valid_password(&mut file)?,
    })
  } else {
    None
  };

  let working_path = match &temp_file {
    Some(t) => t.path(),
    None => file_path,
  };

  println!("[+] Processing file: {}", file_path.display());

  if let Err(e) =
    remove_protection_and_save(working_path, file_path, disable_macros)
  {
    println!("[!] Failed to process {}: {}", file_path.display(), e);
    return Err(e);
  }

  Ok(())
}

fn find_valid_password(file: &mut File) -> Result<NamedTempFile> {
  const MAX_ATTEMPTS: u32 = 3;
  for attempt in 1..=MAX_ATTEMPTS {
    print!(
      "Enter file password (attempt {}/{}): ",
      attempt, MAX_ATTEMPTS
    );
    stdout().flush()?;

    let mut input_pass = String::new();
    stdin().read_line(&mut input_pass)?;
    let input_pass = input_pass.trim();

    if let Ok(temp_file) = attempt_decryption(file, input_pass) {
      return Ok(temp_file);
    }
  }

  bail!("Maximum attempts reached or decryption failed.")
}

fn attempt_decryption(
  file: &mut File,
  password: &str,
) -> Result<NamedTempFile> {
  file.seek(SeekFrom::Start(0))?;

  let temp_file = NamedTempFile::new()?;
  {
    let mut writer = BufWriter::new(&temp_file);
    let file_clone = file.try_clone()?;

    if let Err(e) = decrypt_file(file_clone, &mut writer, password) {
      println!("[!] Password failed: {}", e);
      return Err(e);
    }
    writer.flush()?;
  }

  let path = temp_file.path();
  let verify_file = File::open(path)?;

  if ZipArchive::new(verify_file).is_err() {
    let e = anyhow!("Decryption produced invalid Zip file (wrong password?)");
    println!("[!] Password failed: {}", e);
    return Err(e);
  }

  println!("[+] File decrypted successfully.");
  Ok(temp_file)
}
