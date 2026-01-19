use std::{
  io::{stdin, stdout, Write},
  process::exit,
};

use anyhow::Result;
use clap::Parser;
use ctrlc::set_handler;
use excel_unprotect::{fs::normalize_path, processor::process_file};

#[derive(Parser, Debug)]
#[command(
  author,
  version,
  about = "Excel Protection Remover",
  long_about = "A tool to remove sheet and workbook protection from Excel files (.xlsx, .xlsm)."
)]
struct Args {
  /// Input file paths
  files: Vec<String>,

  /// Password to use if file is encrypted
  #[arg(long, value_name = "PASSWORD")]
  pass: Option<String>,

  /// Keep macros (VBA scripts)
  #[arg(long)]
  keep_macros: bool,
}

fn main() -> Result<()> {
  set_handler(move || {
    exit(0);
  })?;

  let args = Args::parse();
  let mut paths = args.files;
  let interactive = paths.is_empty();

  if interactive {
    print!("Drag Excel file path: ");
    stdout().flush()?;
    let mut input = String::new();
    stdin().read_line(&mut input)?;
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
        eprintln!("[!] Invalid path '{file_path}': {e}");
        has_error = true;
      }
    }
  }

  if interactive || has_error {
    print!("\nPress Enter to exit... ");
    stdout().flush()?;
    let mut _pause = String::new();
    stdin().read_line(&mut _pause)?;
  }

  Ok(())
}
