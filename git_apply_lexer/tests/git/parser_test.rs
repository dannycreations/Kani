use hit::error::Error;
use hit::parser::Line;
use hit::parser::Parser;
use hit::parser::Patch;

#[test]
fn parse_simple_patch() {
  let diff = r#"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-hello world
+Hello, world!
  context
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.old_file, "file.txt");
  assert_eq!(patch.new_file, "file.txt");
  assert_eq!(patch.index_mode, Some(0o100644));
  assert_eq!(patch.hunks.len(), 1);

  let hunk = &patch.hunks[0];
  assert_eq!(hunk.old_line, 1);
  assert_eq!(hunk.old_span, 2);
  assert_eq!(hunk.new_line, 1);
  assert_eq!(hunk.new_span, 2);
  assert_eq!(hunk.lines.len(), 3);
  assert_eq!(hunk.lines[0], Line::Deletion("hello world"));
  assert_eq!(hunk.lines[1], Line::Addition("Hello, world!"));
  assert_eq!(hunk.lines[2], Line::Context("  context"));
}

#[test]
fn parse_rename_with_content_change() {
  let diff = r#"diff --git a/old_name.txt b/new_name.txt
similarity index 80%
rename from old_name.txt
rename to new_name.txt
index 1234567..abcdefg 100644
--- a/old_name.txt
+++ b/new_name.txt
@@ -1 +1 @@
-old content
+new content
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.old_file, "old_name.txt");
  assert_eq!(patch.new_file, "new_name.txt");
  assert_eq!(patch.rename_from.as_deref(), Some("old_name.txt"));
  assert_eq!(patch.rename_to.as_deref(), Some("new_name.txt"));
  assert_eq!(patch.similarity, Some(80));
  assert_eq!(patch.hunks.len(), 1);
}

#[test]
fn parse_new_file_mode() {
  let diff = r#"diff --git a/file.txt b/file.txt
new file mode 100755
index 0000000..1234567
--- /dev/null
+++ b/file.txt
@@ -0,0 +1 @@
+hello
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.new_mode, Some(0o100755));
  assert!(patch.old_mode.is_none());
}

#[test]
fn parse_mode_change_only() {
  let diff = r#"diff --git a/file.txt b/file.txt
old mode 100644
new mode 100755
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();
  assert_eq!(patches.len(), 1);
  let patch = &patches[0];
  assert_eq!(patch.old_mode, Some(0o100644));
  assert_eq!(patch.new_mode, Some(0o100755));
  assert!(patch.hunks.is_empty());
}

#[test]
fn parse_hunk_header_for_new_file() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.hunks.len(), 1);
  let hunk = &patch.hunks[0];
  assert_eq!(hunk.old_line, 0);
  assert_eq!(hunk.old_span, 0);
  assert_eq!(hunk.new_line, 1);
  assert_eq!(hunk.new_span, 3);
  assert_eq!(hunk.lines.len(), 3);
}

#[test]
fn parse_copy_file() {
  let diff = r#"diff --git a/old_file.txt b/new_file.txt
copy from old_file.txt
copy to new_file.txt
dissimilarity index 100%
index 1234567..abcdefg 100644
--- a/old_file.txt
+++ b/new_file.txt
@@ -1 +1 @@
-content
+content
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.old_file, "old_file.txt");
  assert_eq!(patch.new_file, "new_file.txt");
  assert_eq!(patch.copy_from.as_deref(), Some("old_file.txt"));
  assert_eq!(patch.copy_to.as_deref(), Some("new_file.txt"));
  assert_eq!(patch.dissimilarity, Some(100));
}

#[test]
fn parse_multiple_patches() {
  let diff = r#"diff --git a/file1.txt b/file1.txt
index 123..456 100644
--- a/file1.txt
+++ b/file1.txt
@@ -1 +1 @@
-old line 1
+new line 1
diff --git a/file2.txt b/file2.txt
index 789..012 100644
--- a/file2.txt
+++ b/file2.txt
@@ -1 +1 @@
-old line 2
+new line 2
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<Patch>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 2);

  let patch1 = &patches[0];
  assert_eq!(patch1.old_file, "file1.txt");
  assert_eq!(patch1.new_file, "file1.txt");
  assert_eq!(patch1.hunks.len(), 1);

  let patch2 = &patches[1];
  assert_eq!(patch2.old_file, "file2.txt");
  assert_eq!(patch2.new_file, "file2.txt");
  assert_eq!(patch2.hunks.len(), 1);
}

#[test]
fn parse_metadata_only_patch() {
  let diff = r#"diff --git a/file.txt b/file.txt
new file mode 100755
rename from old_file.txt
rename to new_file.txt
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<Patch>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.old_file, "file.txt");
  assert_eq!(patch.new_file, "file.txt");
  assert!(patch.hunks.is_empty());
  assert_eq!(patch.new_mode, Some(0o100755));
  assert_eq!(patch.rename_from.as_deref(), Some("old_file.txt"));
  assert_eq!(patch.rename_to.as_deref(), Some("new_file.txt"));
}

#[test]
fn error_on_invalid_hunk_header_line_counts() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,1 +1,3 @@
-hello
+hello
+world
"#;

  let mut parser = Parser::new(diff);
  let result = parser.next().unwrap();

  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Parse(msg) => {
      assert_eq!(
        msg,
        "Hunk line count mismatch for new file. Expected 3, got 2"
      );
    }
    _ => panic!("Expected Parse error"),
  }
}

#[test]
fn error_on_malformed_file_header() {
  let diff = r#"diff --git a/file.txt"#;
  let mut parser = Parser::new(diff);
  let result = parser.next().unwrap();
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Parse(msg) => assert_eq!(msg, "Invalid file header"),
    _ => panic!("Expected Parse error"),
  }
}

#[test]
fn error_on_malformed_index_line() {
  let diff = r#"diff --git a/file.txt b/file.txt
index 1234567"#;
  let mut parser = Parser::new(diff);
  let result = parser.next().unwrap();
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Parse(msg) => {
      assert_eq!(msg, "Invalid index hash range")
    }
    _ => panic!("Expected Parse error"),
  }
}

#[test]
fn error_on_malformed_hunk_header_missing_lines() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@"#;
  let mut parser = Parser::new(diff);
  let result = parser.next().unwrap();
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Parse(msg) => {
      assert_eq!(
        msg,
        "Hunk line count mismatch for old file. Expected 3, got 0"
      )
    }
    _ => panic!("Expected Parse error"),
  }
}

#[test]
fn error_on_unexpected_line() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
Unexpected line content
@@ -1,3 +1,3 @@
  context 1
-old line
+new line
  context 2
"#;
  let mut parser = Parser::new(diff);
  let result = parser.next().unwrap();
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Parse(msg) => {
      assert_eq!(msg, "Unexpected line: `Unexpected line content`")
    }
    _ => panic!("Expected Parse error"),
  }
}

#[test]
fn parse_patch_without_file_header() {
  let diff = r#"index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-hello world
+Hello, world!
  context
"#;
  let patches = Parser::new(diff)
    .collect::<Result<Vec<_>, Error>>()
    .unwrap();

  assert_eq!(patches.len(), 1);
  let patch = &patches[0];

  assert_eq!(patch.old_file, "file.txt");
  assert_eq!(patch.new_file, "file.txt");
  assert_eq!(patch.index_mode, Some(0o100644));
  assert_eq!(patch.hunks.len(), 1);

  let hunk = &patch.hunks[0];
  assert_eq!(hunk.old_line, 1);
  assert_eq!(hunk.old_span, 2);
  assert_eq!(hunk.new_line, 1);
  assert_eq!(hunk.new_span, 2);
  assert_eq!(hunk.lines.len(), 3);
  assert_eq!(hunk.lines[0], Line::Deletion("hello world"));
  assert_eq!(hunk.lines[1], Line::Addition("Hello, world!"));
  assert_eq!(hunk.lines[2], Line::Context("  context"));
}
