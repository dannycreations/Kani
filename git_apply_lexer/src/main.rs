use clap::Parser as ClapParser;
use hit::applier;
use hit::error::Error;
use hit::fs::OsFileSystem;
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::io::{self, Read};
use std::path::Path;
use std::process;

#[derive(ClapParser)]
#[command(author, version, about, long_about = None)]
struct Cli {
  file: Option<String>,
  #[arg(short, long)]
  reverse: bool,
}

fn run() -> Result<(), Error> {
  let cli = Cli::parse();
  let program_name = env::args().next().unwrap();

  let patch_content = match &cli.file {
    Some(path_str) => {
      let path = Path::new(path_str);
      if !path.exists() && io::stdin().is_terminal() {
        Cli::parse_from(vec![program_name.as_str(), "--help"]);
        return Ok(());
      }

      fs::read_to_string(path_str).map_err(Error::from)?
    }
    None => {
      if io::stdin().is_terminal() {
        Cli::parse_from(vec![program_name.as_str(), "--help"]);
        return Ok(());
      }

      let mut buffer = String::new();
      io::stdin().read_to_string(&mut buffer)?;
      buffer
    }
  };

  let mut fs = OsFileSystem;
  applier::patch(&mut fs, &patch_content, cli.reverse)?;
  Ok(())
}

fn main() {
  match run() {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Error: {}", e);
      process::exit(1);
    }
  }
}
