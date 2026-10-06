use super::IniDocument;

#[test]
fn test_ini_parsing_general() {
  let ini = "\
[section1]
key1 = val1
key2 = val2

[section2]
key3 = val3
";
  let doc = IniDocument::parse(ini);
  assert_eq!(doc.get("section1", "key1"), Some("val1"));
  assert_eq!(doc.get("section1", "key2"), Some("val2"));
  assert_eq!(doc.get("section2", "key3"), Some("val3"));
  assert_eq!(doc.get("section2", "key4"), None);
}
