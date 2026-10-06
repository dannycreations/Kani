use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct IniDocument {
  pub sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl IniDocument {
  pub fn parse(content: &str) -> Self {
    let mut doc = Self::default();
    let mut current_section = String::new();

    for line in content.lines() {
      let line = line.trim();
      if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
        continue;
      }

      if let Some(section) =
        line.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
      {
        current_section = section.trim().to_string();
        continue;
      }

      if let Some(eq_pos) = line.find('=') {
        let key = line[..eq_pos].trim().to_string();
        let val = line[eq_pos + 1..].trim().to_string();
        doc
          .sections
          .entry(current_section.clone())
          .or_default()
          .insert(key, val);
      }
    }

    doc
  }

  pub fn get(&self, section: &str, key: &str) -> Option<&str> {
    self.sections.get(section)?.get(key).map(|s| s.as_str())
  }
}
