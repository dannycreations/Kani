use std::{
  fs::File,
  io::{self, BufWriter, Read, Seek, SeekFrom, Write},
  path::Path,
};

use anyhow::{anyhow, Context, Result};
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
    return Err(anyhow!("File not found"));
  }

  match try_decrypt_and_save(file_path, password) {
    Ok(temp_file) => {
      println!("[+] Processing file: {}", file_path.display());
      let working_path = temp_file.path();
      let result =
        remove_protection_and_save(working_path, file_path, disable_macros);
      if let Err(ref e) = result {
        println!("[!] Failed to process {}: {}", file_path.display(), e);
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
  password: Option<&str>,
) -> Result<NamedTempFile> {
  let mut file = File::open(input_path).context("Failed to open input file")?;

  let mut header = [0u8; 9];
  let n = file.read(&mut header)?;

  if n < 9 || !is_ole_file(&header[..n]) {
    // If not encrypted, we create a temp file copy to work on,
    // to keep the interface consistent (working on a temp file).
    // Or we could just return a temp file that is a copy of original.
    let mut temp = NamedTempFile::new()?;
    file.seek(SeekFrom::Start(0))?;
    io::copy(&mut file, &mut temp)?;
    return Ok(temp);
  }

  println!("[!] File is encrypted with a password-to-open.");

  if let Some(pass) = password {
    attempt_decryption(&mut file, pass)
  } else {
    find_valid_password(&mut file)
  }
}

fn find_valid_password(file: &mut File) -> Result<NamedTempFile> {
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

    if let Ok(temp_file) = attempt_decryption(file, input_pass) {
      return Ok(temp_file);
    }
  }

  Err(anyhow!("Maximum attempts reached or decryption failed."))
}

fn attempt_decryption(
  file: &mut File,
  password: &str,
) -> Result<NamedTempFile> {
  file.seek(SeekFrom::Start(0))?;

  let temp_file = NamedTempFile::new()?;
  // We drop writer before returning temp_file to avoid borrow checker error
  {
    let mut writer = BufWriter::new(&temp_file);

    // We need to clone the file handle because decrypt_file takes ownership or needs independent seek
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
