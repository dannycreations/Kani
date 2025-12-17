use std::io::{self, Write};

use anyhow::Result;
use clap::Parser;
use excel_unprotect::{fs::normalize_path, processor::process_file};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Input file paths
  files: Vec<String>,

  /// Password to use if Excel file is encrypted
  #[arg(long)]
  pass: Option<String>,

  /// Keep macros (VBA scripts)
  #[arg(long)]
  keep_macros: bool,
}

fn main() -> Result<()> {
  ctrlc::set_handler(move || {
    std::process::exit(0);
  })?;

  let args = Args::parse();
  let mut has_error = false;
  let interactive = args.files.is_empty();

  let disable_macros = !args.keep_macros;

  if !interactive {
    for file_path in &args.files {
      if let Ok(path) = normalize_path(file_path) {
        if process_file(&path, args.pass.as_deref(), disable_macros).is_err() {
          has_error = true;
        }
      } else {
        eprintln!("[!] Invalid path: {}", file_path);
        has_error = true;
      }
    }
  } else {
    print!("Enter or Drag Excel file path: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if let Ok(path) = normalize_path(input.trim()) {
      if process_file(&path, args.pass.as_deref(), disable_macros).is_err() {
        has_error = true;
      }
    } else {
      eprintln!("[!] Invalid path provided.");
      has_error = true;
    }
  }

  if interactive || has_error {
    print!("\nPress Enter to exit... ");
    io::stdout().flush()?;
    let mut _pause = String::new();
    io::stdin().read_line(&mut _pause)?;
  }

  Ok(())
}
