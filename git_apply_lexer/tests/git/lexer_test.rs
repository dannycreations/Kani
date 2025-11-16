use hit::error::Error;
use hit::lexer::Lexer;
use hit::lexer::Token;

#[test]
fn lex_simple_diff() {
  let diff = r#"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-hello world
+Hello, world!
   context
"#;
  let mut lexer = Lexer::new(diff);

  assert_eq!(
    lexer.next(),
    Some(Ok(Token::FileHeader {
      old_file: "file.txt",
      new_file: "file.txt"
    }))
  );
  assert_eq!(
    lexer.next(),
    Some(Ok(Token::Index {
      old_hash: "1234567",
      new_hash: "abcdefg",
      mode: Some(0o100644)
    }))
  );
  assert_eq!(lexer.next(), Some(Ok(Token::OldFile("file.txt"))));
  assert_eq!(lexer.next(), Some(Ok(Token::NewFile("file.txt"))));
  assert_eq!(
    lexer.next(),
    Some(Ok(Token::HunkHeader {
      old_line: 1,
      old_span: 2,
      new_line: 1,
      new_span: 2
    }))
  );
  assert_eq!(lexer.next(), Some(Ok(Token::Deletion("hello world"))));
  assert_eq!(lexer.next(), Some(Ok(Token::Addition("Hello, world!"))));
  assert_eq!(lexer.next(), Some(Ok(Token::Context("   context"))));
  assert!(lexer.next().is_none());
}

#[test]
fn lex_no_newline_at_end_of_file() {
  let diff = r#"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-hello
-world
\ No newline at end of file
+hello
+world
\ No newline at end of file
"#;
  let mut lexer = Lexer::new(diff);

  for _ in 0..4 {
    assert!(lexer.next().is_some_and(|r| r.is_ok()));
  }

  assert_eq!(
    lexer.next(),
    Some(Ok(Token::HunkHeader {
      old_line: 1,
      old_span: 2,
      new_line: 1,
      new_span: 2
    }))
  );

  assert_eq!(lexer.next(), Some(Ok(Token::Deletion("hello"))));
  assert_eq!(lexer.next(), Some(Ok(Token::Deletion("world"))));
  assert_eq!(lexer.next(), Some(Ok(Token::NoNewline,)));
  assert_eq!(lexer.next(), Some(Ok(Token::Addition("hello"))));
  assert_eq!(lexer.next(), Some(Ok(Token::Addition("world"))));
  assert_eq!(lexer.next(), Some(Ok(Token::NoNewline,)));

  assert!(lexer.next().is_none());
}

#[test]
fn lex_hunk_header_zero_span() {
  let diff = r#"@@ -0,0 +1,3 @@"#;
  let mut lexer = Lexer::new(diff);
  assert_eq!(
    lexer.next(),
    Some(Ok(Token::HunkHeader {
      old_line: 0,
      old_span: 0,
      new_line: 1,
      new_span: 3
    }))
  );
}

#[test]
fn lex_malformed_git_prefix() {
  let diff = r#"diff --git file.txt b/file.txt"#;
  let mut lexer = Lexer::new(diff);
  let result = lexer.next().unwrap();
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Parse(msg) => assert_eq!(msg, "Malformed file path: `file.txt`"),
    _ => panic!("Expected Parse error"),
  }
}

#[test]
fn lex_rename_file() {
  let diff = r#"rename from old.txt
rename to new.txt
"#;
  let mut lexer = Lexer::new(diff);
  assert_eq!(lexer.next(), Some(Ok(Token::RenameFrom("old.txt"))));
  assert_eq!(lexer.next(), Some(Ok(Token::RenameTo("new.txt"))));
  assert!(lexer.next().is_none());
}

#[test]
fn lex_copy_file() {
  let diff = r#"copy from old.txt
copy to new.txt
"#;
  let mut lexer = Lexer::new(diff);
  assert_eq!(lexer.next(), Some(Ok(Token::CopyFrom("old.txt"))));
  assert_eq!(lexer.next(), Some(Ok(Token::CopyTo("new.txt"))));
  assert!(lexer.next().is_none());
}

#[test]
fn lex_new_file_mode() {
  let diff = "new file mode 100644";
  let mut lexer = Lexer::new(diff);
  assert_eq!(lexer.next(), Some(Ok(Token::NewFileMode(0o100644))));
  assert!(lexer.next().is_none());
}

#[test]
fn lex_deleted_file_mode() {
  let diff = "deleted file mode 100644";
  let mut lexer = Lexer::new(diff);
  assert_eq!(lexer.next(), Some(Ok(Token::DeletedFileMode(0o100644))));
  assert!(lexer.next().is_none());
}

#[test]
fn lex_binary_files_differ() {
  let diff = "Binary files a/old.bin and b/new.bin differ";
  let mut lexer = Lexer::new(diff);
  assert_eq!(
    lexer.next(),
    Some(Ok(Token::BinaryFileDiffer {
      old_file: "a/old.bin",
      new_file: "b/new.bin"
    }))
  );
  assert!(lexer.next().is_none());
}
