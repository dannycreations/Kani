use std::io::{self, Write};

use anyhow::Result;
use clap::Parser;
use excel_unprotect::{fs::normalize_path, processor::process_file};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Input file paths
  files: Vec<String>,

  /// Password to use if file is encrypted
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
  let mut paths = args.files;
  let interactive = paths.is_empty();

  if interactive {
    print!("Drag Excel file path: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if !input.is_empty() {
      paths.push(input.to_string());
    }
  }

  let mut has_error = false;

  for file_path in &paths {
    match normalize_path(file_path) {
      Ok(path) => {
        if process_file(&path, args.pass.as_deref(), !args.keep_macros).is_err()
        {
          has_error = true;
        }
      }
      Err(e) => {
        eprintln!("[!] Invalid path '{}': {}", file_path, e);
        has_error = true;
      }
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
