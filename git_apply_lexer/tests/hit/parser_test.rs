use hit::applier;
use hit::fs::FileSystem;
use hit::fs::MockFileSystem;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn parse_without_hunk_header() {
  let patch_content = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
-hello
+world
"#;
  let source_content = "hello\n";
  let expected_content = "world\n";

  let mut fs = MockFileSystem::new(HashMap::from([(
    PathBuf::from("file.txt"),
    source_content.to_string(),
  )]));

  applier::patch(&mut fs, patch_content, false).unwrap();

  let new_content = fs.read_to_string(&PathBuf::from("file.txt")).unwrap();
  assert_eq!(new_content, expected_content);
}

#[test]
fn parse_without_hunk_header_and_no_file_info() {
  let patch_content = r#"
-hello
+world
"#;
  let source_content = "hello\n";

  let mut fs = MockFileSystem::new(HashMap::from([(
    PathBuf::from("file.txt"),
    source_content.to_string(),
  )]));

  let result = applier::patch(&mut fs, patch_content, false);
  assert!(result.is_err());
}
