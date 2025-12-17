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
}

fn main() -> Result<()> {
  ctrlc::set_handler(move || {
    std::process::exit(0);
  })?;

  let args = Args::parse();

  if !args.files.is_empty() {
    for file_path in &args.files {
      let path = normalize_path(file_path)?;
      process_file(&path, args.pass.as_deref());
    }
  } else {
    print!("Enter or Drag Excel file path: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let path = normalize_path(input.trim())?;
    process_file(&path, args.pass.as_deref());

    print!("\nPress Enter to exit... ");
    io::stdout().flush()?;
    let mut _pause = String::new();
    io::stdin().read_line(&mut _pause)?;
  }

  Ok(())
}
