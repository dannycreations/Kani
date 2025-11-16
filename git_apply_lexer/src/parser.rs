use crate::error::Error;
use crate::lexer::Lexer;
use crate::lexer::Token;
use std::iter::Peekable;

#[derive(Debug, Clone, Copy, PartialEq)]
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

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    let mut patch = Patch::default();

    if let Some(Ok(Token::FileHeader {
      old_file: fh_old,
      new_file: fh_new,
    })) = self.tokens.peek()
    {
      patch.old_file = fh_old;
      patch.new_file = fh_new;
      self.tokens.next();
    }

    while let Some(Ok(token)) = self.tokens.peek() {
      match *token {
        Token::RenameFrom(from) => patch.rename_from = Some(from),
        Token::RenameTo(to) => patch.rename_to = Some(to),
        Token::NewFileMode(mode) => patch.new_mode = Some(mode),
        Token::OldFileMode(mode) => patch.old_mode = Some(mode),
        Token::DeletedFileMode(mode) => patch.deleted_file_mode = Some(mode),
        Token::Similarity(percent) => patch.similarity = Some(percent),
        Token::BinaryFileDiffer { .. } => patch.is_binary = true,
        Token::OldFile(file) => patch.old_file = file,
        Token::NewFile(file) => patch.new_file = file,
        Token::CopyFrom(from) => patch.copy_from = Some(from),
        Token::CopyTo(to) => patch.copy_to = Some(to),
        Token::Dissimilarity(percent) => patch.dissimilarity = Some(percent),
        Token::Index { mode, .. } => patch.index_mode = mode,
        _ => break,
      }
      self.tokens.next();
    }

    if let Some(Err(e)) = self.tokens.peek() {
      return Err(e.clone());
    }

    loop {
      if self
        .tokens
        .peek()
        .is_some_and(|t| matches!(t, Ok(Token::HunkHeader { .. })))
      {
        patch.hunks.push(self.parse_hunk()?);
      } else {
        break;
      }
    }

    if patch.hunks.is_empty() {
      let (lines, old_span, new_span) = self.parse_hunk_lines()?;
      if !lines.is_empty() {
        if patch.old_file.is_empty() && patch.new_file.is_empty() {
          return Err(Error::Parse(
            "Patch has hunks but no file information".into(),
          ));
        }
        patch.hunks.push(Hunk {
          old_line: if old_span > 0 { 1 } else { 0 },
          old_span,
          new_line: if new_span > 0 { 1 } else { 0 },
          new_span,
          lines,
        });
      }
    }

    Ok(patch)
  }

  fn parse_hunk_lines(&mut self) -> Result<(Vec<Line<'a>>, u32, u32), Error> {
    let mut lines = Vec::new();
    let mut old_lines_count = 0;
    let mut new_lines_count = 0;
    while let Some(Ok(token)) = self.tokens.peek() {
      let line = match *token {
        Token::Addition(s) => {
          new_lines_count += 1;
          Line::Addition(s)
        }
        Token::Deletion(s) => {
          old_lines_count += 1;
          Line::Deletion(s)
        }
        Token::Context(s) => {
          old_lines_count += 1;
          new_lines_count += 1;
          Line::Context(s)
        }
        Token::NoNewline => Line::NoNewline,
        _ => break,
      };
      lines.push(line);
      self.tokens.next();
    }

    if let Some(Err(e)) = self.tokens.peek() {
      return Err(e.clone());
    }

    Ok((lines, old_lines_count, new_lines_count))
  }

  fn parse_hunk(&mut self) -> Result<Hunk<'a>, Error> {
    let Some(Ok(Token::HunkHeader {
      old_line,
      old_span,
      new_line,
      new_span,
    })) = self.tokens.next()
    else {
      return Err(Error::Parse("Expected hunk header".into()));
    };

    let (lines, old_lines_count, new_lines_count) = self.parse_hunk_lines()?;

    if old_lines_count != old_span {
      return Err(Error::Parse(
        format!(
          "Hunk line count mismatch for old file. Expected {}, got {}",
          old_span, old_lines_count
        )
        .into(),
      ));
    }

    if new_lines_count != new_span {
      return Err(Error::Parse(
        format!(
          "Hunk line count mismatch for new file. Expected {}, got {}",
          new_span, new_lines_count
        )
        .into(),
      ));
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
    self.tokens.peek()?;
    Some(self.parse_patch())
  }
}
