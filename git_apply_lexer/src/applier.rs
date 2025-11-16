use crate::error::Error;
use crate::fs::FileSystem;
use crate::parser::Hunk;
use crate::parser::Line;
use crate::parser::Parser;
use crate::parser::Patch;
#[cfg(unix)]
use std::fs::Permissions;
use std::io;
use std::mem;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

impl<'a> Patch<'a> {
  pub(crate) fn invert(mut self) -> Self {
    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.rename_from, &mut self.rename_to);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.old_mode, &mut self.new_mode);
    if self.new_file == "/dev/null" {
      self.new_mode = self.deleted_file_mode;
    }

    self.hunks.iter_mut().for_each(Hunk::invert);
    self
  }
}

impl<'a> Hunk<'a> {
  pub(crate) fn invert(&mut self) {
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
    self.lines.iter_mut().for_each(|line| {
      *line = match *line {
        Line::Addition(s) => Line::Deletion(s),
        Line::Deletion(s) => Line::Addition(s),
        other => other,
      };
    });
  }
}

pub fn apply<'a>(patch: &Patch<'a>, source: &'a str) -> Result<String, Error> {
  if patch.hunks.is_empty() {
    return Ok(source.to_string());
  }

  let source_lines = source.split('\n');
  let mut result_lines = Vec::new();
  let mut source_iter = source_lines.peekable();

  let mut current_source_line_num: usize = 1;
  let mut new_file_should_have_no_newline = false;

  for hunk in &patch.hunks {
    while current_source_line_num < hunk.old_line as usize {
      match source_iter.next() {
        Some(line) => {
          result_lines.push(line);
          current_source_line_num += 1;
        }
        None => {
          return Err(Error::Apply(format!(
            "Unexpected EOF while seeking to line {}",
            hunk.old_line
          )));
        }
      }
    }

    let mut in_addition_block = false;
    for line in &hunk.lines {
      match line {
        Line::Addition(text) => {
          in_addition_block = true;
          result_lines.push(text);
          new_file_should_have_no_newline = false;
        }
        Line::Context(text) | Line::Deletion(text) => {
          in_addition_block = false;
          let source_line = *source_iter.peek().ok_or_else(|| {
            Error::Apply(format!(
              "Patch mismatch at line {}. Expected: `{}`, Found: `<EOF>`",
              current_source_line_num, text
            ))
          })?;

          if source_line != *text {
            return Err(Error::Apply(format!(
              "Patch mismatch at line {}. Expected: `{}`, Found: `{}`",
              current_source_line_num, text, source_line
            )));
          }

          let consumed_line = source_iter.next().unwrap();
          if let Line::Context(_) = line {
            result_lines.push(consumed_line);
            new_file_should_have_no_newline = false;
          }

          current_source_line_num += 1;
        }
        Line::NoNewline => {
          if !in_addition_block && source_iter.peek().is_some() {
            return Err(Error::Apply(format!(
              "Patch mismatch at line {}. Expected end of file, Found: ``",
              current_source_line_num
            )));
          }
          new_file_should_have_no_newline = true;
        }
      }
    }
  }

  result_lines.extend(source_iter);

  if result_lines.is_empty() {
    return Ok(String::new());
  }

  let mut final_output = result_lines.join("\n");

  if new_file_should_have_no_newline {
    if final_output.ends_with('\n') {
      final_output.pop();
    }
  } else if !final_output.is_empty() && !final_output.ends_with('\n') {
    final_output.push('\n');
  }

  Ok(final_output)
}

pub fn patch(
  fs: &mut impl FileSystem,
  patch_content: &str,
  reverse: bool,
) -> Result<(), Error> {
  for patch_result in Parser::new(patch_content) {
    let patch = patch_result?;
    let patch = if reverse { patch.invert() } else { patch };

    if patch.is_binary {
      return Err(Error::Unsupported("Binary files are not supported".into()));
    }

    let source_path = Path::new(patch.old_file);
    let source_content = if patch.old_file == "/dev/null" {
      String::new()
    } else {
      let path_to_read = patch.copy_from.map_or(source_path, Path::new);
      match fs.read_to_string(path_to_read) {
        Ok(content) => content,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
      }
    };

    let new_content = apply(&patch, &source_content)?;

    let output_path = Path::new(patch.new_file);
    if patch.new_file == "/dev/null" {
      match fs.remove_file(source_path) {
        Ok(()) => println!("Deleted file: {}", source_path.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
      }
    } else {
      if let Some(parent) = output_path.parent() {
        fs.create_dir_all(parent)?;
      }

      fs.write(output_path, &new_content)?;
      println!("Applied patch to: {}", output_path.display());

      #[cfg(unix)]
      {
        if let Some(mode) = patch.new_mode.or(patch.index_mode) {
          let perms = Permissions::from_mode(mode);
          fs.set_permissions(output_path, perms)?;
        }
      }

      if patch.rename_from.is_some() && source_path != output_path {
        match fs.remove_file(source_path) {
          Ok(()) => {}
          Err(e) if e.kind() == io::ErrorKind::NotFound => {}
          Err(e) => return Err(e.into()),
        }
      }
    }
  }

  Ok(())
}
