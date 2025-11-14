use crate::error::Error;
use crate::lexer::{Lexer, Token};
use std::iter::Peekable;

#[derive(Debug, PartialEq)]
pub enum Line<'a> {
  Addition(&'a str),
  Deletion(&'a str),
  Context(&'a str),
  NoNewline,
}

#[derive(Debug, PartialEq, Default)]
pub struct Hunk<'a> {
  pub old_line: u32,
  pub old_span: u32,
  pub new_line: u32,
  pub new_span: u32,
  pub lines: Vec<Line<'a>>,
}

#[derive(Debug, PartialEq, Default)]
pub struct Patch<'a> {
  pub old_file: &'a str,
  pub new_file: &'a str,
  pub hunks: Vec<Hunk<'a>>,
  pub rename_from: Option<&'a str>,
  pub rename_to: Option<&'a str>,
  pub new_mode: Option<u32>,
  pub old_mode: Option<u32>,
  pub deleted_file_mode: Option<u32>,
  pub similarity: Option<u32>,
  pub is_binary: bool,
  pub copy_from: Option<&'a str>,
  pub copy_to: Option<&'a str>,
  pub dissimilarity: Option<u32>,
  pub index_mode: Option<u32>,
}

pub struct Parser<'a> {
  tokens: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
  pub fn new(source: &'a str) -> Self {
    Self {
      tokens: Lexer::new(source).peekable(),
    }
  }

  fn next_token(&mut self) -> Result<Token<'a>, Error> {
    self
      .tokens
      .next()
      .transpose()?
      .ok_or_else(|| Error::Parse("Unexpected EOF".into()))
  }

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    let mut old_file = "";
    let mut new_file = "";

    if let Some(Ok(Token::FileHeader {
      old_file: fh_old,
      new_file: fh_new,
    })) = self.tokens.peek()
    {
      old_file = fh_old;
      new_file = fh_new;
      self.next_token()?;
    }

    let mut rename_from = None;
    let mut rename_to = None;
    let mut new_mode = None;
    let mut old_mode = None;
    let mut deleted_file_mode = None;
    let mut similarity = None;
    let mut is_binary = false;
    let mut copy_from = None;
    let mut copy_to = None;
    let mut dissimilarity = None;
    let mut index_mode = None;

    loop {
      match self.tokens.peek() {
        Some(Err(_)) => {
          return self.next_token().map(|_| unreachable!());
        }
        Some(Ok(
          Token::RenameFrom(_)
          | Token::RenameTo(_)
          | Token::NewFileMode(_)
          | Token::OldFileMode(_)
          | Token::DeletedFileMode(_)
          | Token::Similarity(_)
          | Token::BinaryFileDiffer { .. }
          | Token::OldFile(_)
          | Token::NewFile(_)
          | Token::CopyFrom(_)
          | Token::CopyTo(_)
          | Token::Dissimilarity(_)
          | Token::Index { .. },
        )) => {
          let token = self.next_token()?;
          match token {
            Token::RenameFrom(from) => rename_from = Some(from),
            Token::RenameTo(to) => rename_to = Some(to),
            Token::NewFileMode(mode) => new_mode = Some(mode),
            Token::OldFileMode(mode) => old_mode = Some(mode),
            Token::DeletedFileMode(mode) => deleted_file_mode = Some(mode),
            Token::Similarity(percent) => similarity = Some(percent),
            Token::BinaryFileDiffer { .. } => is_binary = true,
            Token::OldFile(file) => old_file = file,
            Token::NewFile(file) => new_file = file,
            Token::CopyFrom(from) => copy_from = Some(from),
            Token::CopyTo(to) => copy_to = Some(to),
            Token::Dissimilarity(percent) => dissimilarity = Some(percent),
            Token::Index { mode, .. } => index_mode = mode,
            _ => unreachable!(),
          }
        }
        _ => break,
      }
    }

    let mut hunks = Vec::new();
    while let Some(Ok(Token::HunkHeader { .. })) = self.tokens.peek() {
      hunks.push(self.parse_hunk()?);
    }

    Ok(Patch {
      old_file,
      new_file,
      hunks,
      rename_from,
      rename_to,
      new_mode,
      old_mode,
      deleted_file_mode,
      similarity,
      is_binary,
      copy_from,
      copy_to,
      dissimilarity,
      index_mode,
    })
  }

  fn parse_hunk(&mut self) -> Result<Hunk<'a>, Error> {
    let (old_line, old_span, new_line, new_span) = match self.next_token()? {
      Token::HunkHeader {
        old_line,
        old_span,
        new_line,
        new_span,
      } => (old_line, old_span, new_line, new_span),
      other => {
        return Err(Error::Parse(format!(
          "Expected hunk header, found `{:?}`",
          other
        )));
      }
    };

    let mut lines = Vec::new();
    loop {
      match self.tokens.peek() {
        Some(Err(_)) => {
          return self.next_token().map(|_| unreachable!());
        }
        Some(Ok(token_ref)) => match token_ref {
          Token::Addition(line_content) => {
            lines.push(Line::Addition(line_content));
            self.next_token()?;
          }
          Token::Deletion(line_content) => {
            lines.push(Line::Deletion(line_content));
            self.next_token()?;
          }
          Token::Context(line_content) => {
            lines.push(Line::Context(line_content));
            self.next_token()?;
          }
          Token::NoNewline => {
            lines.push(Line::NoNewline);
            self.next_token()?;
          }
          _ => break,
        },
        None => break,
      }
    }

    let old_lines_count = lines
      .iter()
      .filter(|l| matches!(l, Line::Context(_) | Line::Deletion(_)))
      .count();
    let new_lines_count = lines
      .iter()
      .filter(|l| matches!(l, Line::Context(_) | Line::Addition(_)))
      .count();

    if old_lines_count != old_span as usize {
      return Err(Error::Parse(format!(
        "Hunk line count mismatch for old file. Expected {}, got {}",
        old_span, old_lines_count
      )));
    }

    if new_lines_count != new_span as usize {
      return Err(Error::Parse(format!(
        "Hunk line count mismatch for new file. Expected {}, got {}",
        new_span, new_lines_count
      )));
    }

    Ok(Hunk {
      old_line,
      old_span,
      new_line,
      new_span,
      lines,
    })
  }
}

impl<'a> Iterator for Parser<'a> {
  type Item = Result<Patch<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.tokens.peek() {
      Some(Err(_)) => {
        let err = self.tokens.next();
        return Some(Err(err.unwrap().unwrap_err()));
      }
      None => return None,
      _ => {}
    }
    Some(self.parse_patch())
  }
}
