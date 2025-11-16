use clap::CommandFactory;
use clap::Parser;
use hit::applier;
use hit::error::Error;
use hit::fs::OsFileSystem;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::io::Read;
use std::process;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
  file: Option<String>,
  #[arg(short, long)]
  reverse: bool,
}

fn run() -> Result<(), Error> {
  let cli = Cli::parse();

  let patch_content = if let Some(path_str) = cli.file {
    fs::read_to_string(path_str)?
  } else {
    if io::stdin().is_terminal() {
      Cli::command().print_help().map_err(Error::from)?;
      return Ok(());
    }
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    buffer
  };

  applier::patch(&mut OsFileSystem, &patch_content, cli.reverse)?;
  Ok(())
}

fn main() {
  if let Err(e) = run() {
    eprintln!("Error: {}", e);
    process::exit(1);
  }
}
