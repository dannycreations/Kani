use crate::error::Error;
use std::iter::Peekable;
use std::str::Lines;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
  FileHeader {
    old_file: &'a str,
    new_file: &'a str,
  },
  Index {
    old_hash: &'a str,
    new_hash: &'a str,
    mode: Option<u32>,
  },
  OldFile(&'a str),
  NewFile(&'a str),
  HunkHeader {
    old_line: u32,
    old_span: u32,
    new_line: u32,
    new_span: u32,
  },
  Addition(&'a str),
  Deletion(&'a str),
  Context(&'a str),
  NoNewline,
  RenameFrom(&'a str),
  RenameTo(&'a str),
  Similarity(u32),
  NewFileMode(u32),
  OldFileMode(u32),
  DeletedFileMode(u32),
  BinaryFileDiffer {
    old_file: &'a str,
    new_file: &'a str,
  },
  CopyFrom(&'a str),
  CopyTo(&'a str),
  Dissimilarity(u32),
}

pub struct Lexer<'a> {
  lines: Peekable<Lines<'a>>,
}

impl<'a> Lexer<'a> {
  pub fn new(source: &'a str) -> Self {
    Lexer {
      lines: source.lines().peekable(),
    }
  }

  fn strip_git_prefix(s: &'a str) -> Result<&'a str, Error> {
    s.strip_prefix("a/")
      .or_else(|| s.strip_prefix("b/"))
      .or_else(|| (s == "/dev/null").then_some(s))
      .ok_or_else(|| {
        Error::Parse(format!("Malformed file path: `{}`", s).into())
      })
  }

  fn parse_index_line(rest: &'a str) -> Result<Token<'a>, Error> {
    let mut parts = rest.split_whitespace();
    let hashes = parts
      .next()
      .ok_or(Error::Parse("Invalid index line".into()))?;

    let (old_hash, new_hash) = hashes
      .split_once("..")
      .ok_or(Error::Parse("Invalid index hash range".into()))?;
    let mode = parts.next().map(Self::parse_octal_mode).transpose()?;

    Ok(Token::Index {
      old_hash,
      new_hash,
      mode,
    })
  }

  fn parse_hunk_header(header: &'a str) -> Result<Token<'a>, Error> {
    let content = header
      .split(" @@")
      .next()
      .ok_or(Error::Parse("Malformed hunk header".into()))?;

    let mut parts = content.split_whitespace();

    let old_range_str = parts
      .next()
      .and_then(|s| s.strip_prefix('-'))
      .ok_or(Error::Parse("Missing old range in hunk header".into()))?;

    let new_range_str = parts
      .next()
      .and_then(|s| s.strip_prefix('+'))
      .ok_or(Error::Parse("Missing new range in hunk header".into()))?;

    let (old_line, old_span) = Self::parse_range(old_range_str)?;
    let (new_line, new_span) = Self::parse_range(new_range_str)?;

    Ok(Token::HunkHeader {
      old_line,
      old_span,
      new_line,
      new_span,
    })
  }

  fn parse_range(range_str: &str) -> Result<(u32, u32), Error> {
    let (line_str, span_str) =
      range_str.split_once(',').unwrap_or((range_str, "1"));

    let line = line_str.parse().map_err(|e| {
      Error::Parse(
        format!("Invalid hunk range line: `{}` - {}", range_str, e).into(),
      )
    })?;

    let span = span_str.parse().map_err(|e| {
      Error::Parse(
        format!("Invalid hunk range span: `{}` - {}", range_str, e).into(),
      )
    })?;

    Ok((line, span))
  }

  fn parse_percentage(
    s: &'a str,
    error_msg: &'static str,
  ) -> Result<u32, Error> {
    s.strip_suffix('%')
      .ok_or_else(|| {
        Error::Parse(format!("{}: Missing percentage sign", error_msg).into())
      })?
      .parse()
      .map_err(|e| Error::Parse(format!("{}: {}", error_msg, e).into()))
  }

  fn parse_octal_mode(s: &str) -> Result<u32, Error> {
    u32::from_str_radix(s, 8)
      .map_err(|e| Error::Parse(format!("Invalid file mode: {}", e).into()))
  }

  fn next_token(&mut self) -> Result<Token<'a>, Error> {
    let line_content = self
      .lines
      .next()
      .ok_or(Error::Parse("Unexpected EOF".into()))?;

    if let Some(rest) = line_content.strip_prefix("diff --git ") {
      let mut parts = rest.split_whitespace();
      match (parts.next(), parts.next()) {
        (Some(old_file_raw), Some(new_file_raw)) => {
          let old_file = Self::strip_git_prefix(old_file_raw)?;
          let new_file = Self::strip_git_prefix(new_file_raw)?;
          Ok(Token::FileHeader { old_file, new_file })
        }
        _ => Err(Error::Parse("Invalid file header".into())),
      }
    } else if let Some(rest) = line_content.strip_prefix("deleted file mode ") {
      let mode = Self::parse_octal_mode(rest)?;
      Ok(Token::DeletedFileMode(mode))
    } else if let Some(rest) = line_content.strip_prefix("dissimilarity index ")
    {
      let percent = Self::parse_percentage(rest, "Invalid dissimilarity")?;
      Ok(Token::Dissimilarity(percent))
    } else if let Some(rest) = line_content.strip_prefix("index ") {
      Self::parse_index_line(rest)
    } else if let Some(stripped) = line_content.strip_prefix("--- ") {
      Ok(Token::OldFile(Self::strip_git_prefix(stripped)?))
    } else if let Some(stripped) = line_content.strip_prefix("+++ ") {
      Ok(Token::NewFile(Self::strip_git_prefix(stripped)?))
    } else if let Some(stripped) = line_content.strip_prefix('-') {
      Ok(Token::Deletion(stripped))
    } else if let Some(stripped) = line_content.strip_prefix('+') {
      Ok(Token::Addition(stripped))
    } else if let Some(hunk_header) = line_content.strip_prefix("@@ ") {
      Self::parse_hunk_header(hunk_header)
    } else if line_content.starts_with('@') {
      Err(Error::Parse(
        format!("Unexpected line: `{}`", line_content).into(),
      ))
    } else if line_content.starts_with(' ') {
      Ok(Token::Context(line_content))
    } else if line_content == "\\ No newline at end of file" {
      Ok(Token::NoNewline)
    } else if let Some(rest) = line_content.strip_prefix("rename from ") {
      Ok(Token::RenameFrom(rest))
    } else if let Some(rest) = line_content.strip_prefix("rename to ") {
      Ok(Token::RenameTo(rest))
    } else if let Some(rest) = line_content.strip_prefix("similarity index ") {
      let percent = Self::parse_percentage(rest, "Invalid similarity")?;
      Ok(Token::Similarity(percent))
    } else if let Some(rest) = line_content
      .strip_prefix("new file mode ")
      .or_else(|| line_content.strip_prefix("new mode "))
    {
      let mode = Self::parse_octal_mode(rest)?;
      Ok(Token::NewFileMode(mode))
    } else if let Some(rest) = line_content.strip_prefix("old mode ") {
      let mode = Self::parse_octal_mode(rest)?;
      Ok(Token::OldFileMode(mode))
    } else if let Some(rest) = line_content.strip_prefix("Binary files ") {
      let (old_file, rest) = rest
        .split_once(" and ")
        .ok_or(Error::Parse("Invalid binary files line".into()))?;
      let new_file = rest
        .strip_suffix(" differ")
        .ok_or(Error::Parse("Invalid binary files line".into()))?;
      Ok(Token::BinaryFileDiffer { old_file, new_file })
    } else if let Some(rest) = line_content.strip_prefix("copy from ") {
      Ok(Token::CopyFrom(rest))
    } else if let Some(rest) = line_content.strip_prefix("copy to ") {
      Ok(Token::CopyTo(rest))
    } else if line_content.is_empty() {
      Ok(Token::Context(""))
    } else {
      Err(Error::Parse(
        format!("Unexpected line: `{}`", line_content).into(),
      ))
    }
  }
}

impl<'a> Iterator for Lexer<'a> {
  type Item = Result<Token<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    while let Some(&"") = self.lines.peek() {
      self.lines.next();
    }
    self.lines.peek()?;
    Some(self.next_token())
  }
}
