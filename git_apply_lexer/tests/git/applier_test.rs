use hit::applier;
use hit::error::Error;
use hit::fs::FileSystem;
use hit::fs::MockFileSystem;
use hit::parser::Hunk;
use hit::parser::Line;
use hit::parser::Patch;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn apply_simple_patch() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 3,
      new_line: 1,
      new_span: 3,
      lines: vec![
        Line::Context("  context 1"),
        Line::Deletion("old line"),
        Line::Addition("new line"),
        Line::Context("  context 2"),
      ],
    }],
    ..Default::default()
  };
  let source = "  context 1\nold line\n  context 2\n";
  let expected = "  context 1\nnew line\n  context 2\n";

  let result = applier::apply(&patch, source).unwrap();
  assert_eq!(result, expected);
}

#[test]
fn apply_removes_trailing_newline() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 2,
      new_line: 1,
      new_span: 2,
      lines: vec![
        Line::Deletion("line1"),
        Line::Deletion("line2"),
        Line::Addition("Line1_Changed"),
        Line::Addition("line2"),
        Line::NoNewline,
      ],
    }],
    ..Default::default()
  };
  let source = "line1\nline2\n";
  let expected = "Line1_Changed\nline2";
  assert_eq!(applier::apply(&patch, source).unwrap(), expected);
}

#[test]
fn apply_adds_trailing_newline() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 1,
      new_line: 1,
      new_span: 2,
      lines: vec![
        Line::Deletion("hello"),
        Line::Addition("hello"),
        Line::Addition("world"),
      ],
    }],
    ..Default::default()
  };
  let source = "hello";
  let expected = "hello\nworld\n";
  assert_eq!(applier::apply(&patch, source).unwrap(), expected);
}

#[test]
fn apply_preserves_and_adds_trailing_newline() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 2,
      new_line: 1,
      new_span: 3,
      lines: vec![
        Line::Deletion("line1"),
        Line::Deletion("line2"),
        Line::NoNewline,
        Line::Addition("line1"),
        Line::Addition("line2"),
        Line::Addition("line3"),
      ],
    }],
    ..Default::default()
  };
  let source = "line1\nline2";
  let expected = "line1\nline2\nline3\n";
  assert_eq!(applier::apply(&patch, source).unwrap(), expected);
}

#[test]
fn apply_mismatch_on_unexpected_trailing_newline() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 1,
      new_line: 1,
      new_span: 1,
      lines: vec![
        Line::Deletion("hello"),
        Line::NoNewline,
        Line::Addition("world"),
      ],
    }],
    ..Default::default()
  };
  let source = "hello\n";
  let result = applier::apply(&patch, source);
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Apply(msg) => {
      assert_eq!(
        msg,
        "Patch mismatch at line 2. Expected end of file, Found: ``"
      );
    }
    _ => panic!("Expected Apply error"),
  }
}

#[test]
fn patch_rename_file_with_content_change() {
  let diff = r#"diff --git a/old_name.txt b/new_name.txt
similarity index 80%
rename from old_name.txt
rename to new_name.txt
--- a/old_name.txt
+++ b/new_name.txt
@@ -1 +1 @@
-file content
+new file content
"#;

  let mut files = HashMap::new();
  files.insert(PathBuf::from("old_name.txt"), "file content\n".to_string());
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(!fs.files.contains_key(&PathBuf::from("old_name.txt")));
  assert!(fs.files.contains_key(&PathBuf::from("new_name.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("new_name.txt")).unwrap(),
    "new file content\n"
  );
}

#[test]
fn patch_rename_file_metadata_only() {
  let diff = r#"diff --git a/old_metadata.txt b/new_metadata.txt
similarity index 100%
rename from old_metadata.txt
rename to new_metadata.txt
"#;

  let mut files = HashMap::new();
  files.insert(PathBuf::from("old_metadata.txt"), "content".to_string());
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(!fs.files.contains_key(&PathBuf::from("old_metadata.txt")));
  assert!(fs.files.contains_key(&PathBuf::from("new_metadata.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("new_metadata.txt"))
      .unwrap(),
    "content"
  );
}

#[test]
fn apply_patch_mismatch() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 1,
      new_line: 1,
      new_span: 1,
      lines: vec![Line::Context("expected line")],
    }],
    ..Default::default()
  };

  let source = "different line";
  let result = applier::apply(&patch, source);
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Apply(msg) => {
      assert_eq!(
        msg,
        "Patch mismatch at line 1. Expected: `expected line`, Found: `different line`"
      );
    }
    _ => panic!("Expected Apply error"),
  }
}

#[test]
fn patch_create_file() {
  let diff = r#"diff --git a/new_file.txt b/new_file.txt
new file mode 100644
index 0000000..abcdef0
--- /dev/null
+++ b/new_file.txt
@@ -0,0 +1,2 @@
+line 1
+line 2
"#;

  let mut fs = MockFileSystem::new(HashMap::new());

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(fs.files.contains_key(&PathBuf::from("new_file.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("new_file.txt")).unwrap(),
    "line 1\nline 2\n"
  );
}

#[test]
fn patch_delete_file() {
  let diff = r#"diff --git a/file_to_delete.txt b/file_to_delete.txt
index abcdef0..0000000
--- a/file_to_delete.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-line 1
-line 2
"#;

  let mut files = HashMap::new();
  files.insert(
    PathBuf::from("file_to_delete.txt"),
    "line 1\nline 2\n".to_string(),
  );
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(!fs.files.contains_key(&PathBuf::from("file_to_delete.txt")));
}

#[test]
fn apply_multiple_hunks() {
  let diff = r#"diff --git a/file.txt b/file.txt
index abcdef0..abcdef0
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-line 1
-line 2
+new line 1
+new line 2
@@ -4,2 +4,2 @@
-line 4
-line 5
+new line 4
+new line 5
"#;

  let mut files = HashMap::new();
  files.insert(
    PathBuf::from("file.txt"),
    "line 1\nline 2\nline 3\nline 4\nline 5\n".to_string(),
  );
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert_eq!(
    fs.read_to_string(&PathBuf::from("file.txt")).unwrap(),
    "new line 1\nnew line 2\nline 3\nnew line 4\nnew line 5\n"
  );
}

#[test]
fn patch_copy_file() {
  let diff = r#"diff --git a/old_file.txt b/new_file.txt
copy from old_file.txt
copy to new_file.txt
"#;

  let mut files = HashMap::new();
  files.insert(PathBuf::from("old_file.txt"), "content".to_string());
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(fs.files.contains_key(&PathBuf::from("old_file.txt")));
  assert!(fs.files.contains_key(&PathBuf::from("new_file.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("new_file.txt")).unwrap(),
    "content"
  );
}

#[test]
#[cfg(unix)]
fn patch_change_file_mode() {
  use std::os::unix::fs::PermissionsExt;
  let diff = r#"diff --git a/file.txt b/file.txt
old mode 100644
new mode 100755
"#;

  let mut files = HashMap::new();
  files.insert(PathBuf::from("file.txt"), "hello\n".to_string());
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(fs.files.contains_key(&PathBuf::from("file.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("file.txt")).unwrap(),
    "hello\n"
  );
  assert_eq!(
    fs.get_permissions(&PathBuf::from("file.txt"))
      .unwrap()
      .mode(),
    0o100755
  );
}

#[test]
#[cfg(not(unix))]
fn patch_change_file_mode_unsupported() {
  let diff = r#"diff --git a/file.txt b/file.txt
old mode 100644
new mode 100755
"#;
  let mut files = HashMap::new();
  files.insert(PathBuf::from("file.txt"), "hello\n".to_string());
  let mut fs = MockFileSystem::new(files);
  applier::patch(&mut fs, diff, false).unwrap();
  assert_eq!(
    fs.read_to_string(&PathBuf::from("file.txt")).unwrap(),
    "hello\n"
  );
}

#[test]
fn apply_empty_lines() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 5,
      new_line: 1,
      new_span: 5,
      lines: vec![
        Line::Context(" line 1"),
        Line::Context(" "),
        Line::Context(" line 3"),
        Line::Deletion("line 4"),
        Line::Addition("new line 4"),
        Line::Context(" line 5"),
      ],
    }],
    ..Default::default()
  };
  let source = " line 1\n \n line 3\nline 4\n line 5\n";
  let expected = " line 1\n \n line 3\nnew line 4\n line 5\n";

  let result = applier::apply(&patch, source).unwrap();
  assert_eq!(result, expected);
}

#[test]
fn patch_whitespace_context_mismatch() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
   context line
-deletion line
+addition line
"#;
  let mut files = HashMap::new();
  files.insert(
    PathBuf::from("file.txt"),
    "  context line\ndeletion line\n".to_string(),
  );
  let mut fs = MockFileSystem::new(files);
  let result = applier::patch(&mut fs, diff, false);
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Apply(msg) => assert_eq!(
      msg,
      "Patch mismatch at line 1. Expected: `   context line`, Found: `  context line`"
    ),
    e => panic!("Expected Apply error, got {:?}", e),
  }
}

#[test]
fn patch_whitespace_deletion_mismatch() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 context line
-  deletion line
+addition line
"#;
  let mut files = HashMap::new();
  files.insert(
    PathBuf::from("file.txt"),
    " context line\n   deletion line\n".to_string(),
  );
  let mut fs = MockFileSystem::new(files);
  let result = applier::patch(&mut fs, diff, false);
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Apply(msg) => assert_eq!(
      msg,
      "Patch mismatch at line 2. Expected: `  deletion line`, Found: `   deletion line`"
    ),
    e => panic!("Expected Apply error, got {:?}", e),
  }
}

#[test]
fn patch_whitespace_match() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
  context line
-  deletion line
+  addition line
"#;
  let mut files = HashMap::new();
  files.insert(
    PathBuf::from("file.txt"),
    "  context line\n  deletion line\n".to_string(),
  );
  let mut fs = MockFileSystem::new(files);
  applier::patch(&mut fs, diff, false).unwrap();
  assert_eq!(
    fs.read_to_string(&PathBuf::from("file.txt")).unwrap(),
    "  context line\n  addition line\n"
  );
}

#[test]
fn patch_binary_file_unsupported() {
  let diff = r#"diff --git a/image.png b/image.png
new file mode 100644
index 0000000..8989898
Binary files /dev/null and b/image.png differ
"#;

  let mut fs = MockFileSystem::new(HashMap::new());

  let result = applier::patch(&mut fs, diff, false);
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Unsupported(msg) => {
      assert_eq!(msg, "Binary files are not supported");
    }
    err => panic!("Expected Unsupported error, got {:?}", err),
  }
}

#[test]
fn patch_reverse() {
  let diff = r#"diff --git a/file.txt b/file.txt
index 1234567..abcdefg
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
  context 1
-old line
+new line
  context 2
"#;
  let initial_content = "  context 1\nnew line\n  context 2\n";
  let expected_content = "  context 1\nold line\n  context 2\n";

  let mut files = HashMap::new();
  files.insert(PathBuf::from("file.txt"), initial_content.to_string());
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, true).unwrap();

  assert_eq!(
    fs.read_to_string(&PathBuf::from("file.txt")).unwrap(),
    expected_content
  );
}

#[test]
fn patch_create_file_in_new_directory() {
  let diff = r#"diff --git a/new/dir/file.txt b/new/dir/file.txt
new file mode 100644
index 0000000..abcdef0
--- /dev/null
+++ b/new/dir/file.txt
@@ -0,0 +1 @@
+hello world
"#;

  let mut fs = MockFileSystem::new(HashMap::new());

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(fs.files.contains_key(&PathBuf::from("new/dir/file.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("new/dir/file.txt"))
      .unwrap(),
    "hello world\n"
  );
  assert!(fs.created_dirs.contains(&PathBuf::from("new/dir")));
}

#[test]
fn patch_with_offset_line_numbers() {
  let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -10,7 +10,7 @@
 some context
 some more context
 a final bit of context
-the line to remove
+the new line to add
 and more context
 and more context
 and a final context
"#;
  let mut files = HashMap::new();
  let source = "line 1\nline 2\nsome context\nsome more context\na final bit of context\nthe line to remove\nand more context\nand more context\nand a final context\nline 10\n";
  files.insert(PathBuf::from("file.txt"), source.to_string());
  let mut fs = MockFileSystem::new(files);

  let result = applier::patch(&mut fs, diff, false);
  assert!(result.is_err());
  match result.unwrap_err() {
    Error::Apply(msg) => {
      assert_eq!(
        msg,
        "Patch mismatch at line 10. Expected: ` some context`, Found: `line 10`"
      );
    }
    e => panic!("Expected Apply error, got {:?}", e),
  }
}

#[test]
fn apply_only_context_lines() {
  let patch = Patch {
    old_file: "file.txt",
    new_file: "file.txt",
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 3,
      new_line: 1,
      new_span: 3,
      lines: vec![
        Line::Context("  context 1"),
        Line::Context("  context 2"),
        Line::Context("  context 3"),
      ],
    }],
    ..Default::default()
  };
  let source = "  context 1\n  context 2\n  context 3\n";
  let expected = "  context 1\n  context 2\n  context 3\n";

  let result = applier::apply(&patch, source).unwrap();
  assert_eq!(result, expected);
}

#[test]
fn patch_apply_to_empty_file() {
  let diff = r#"diff --git a/empty.txt b/empty.txt
index 0000000..abcdef0 100644
--- a/empty.txt
+++ b/empty.txt
@@ -0,0 +1,2 @@
+line 1
+line 2
"#;
  let mut files = HashMap::new();
  files.insert(PathBuf::from("empty.txt"), "".to_string());
  let mut fs = MockFileSystem::new(files);

  applier::patch(&mut fs, diff, false).unwrap();
  assert!(fs.files.contains_key(&PathBuf::from("empty.txt")));
  assert_eq!(
    fs.read_to_string(&PathBuf::from("empty.txt")).unwrap(),
    "line 1\nline 2\n"
  );
}
