use quick_xml::{
  events::{attributes::Attribute, BytesStart},
  XmlVersion,
};

pub fn normalized_owned(a: &Attribute) -> quick_xml::Result<String> {
  a.normalized_value(XmlVersion::Implicit1_0)
    .map(|v| v.into_owned())
}

pub fn find_attr(e: &BytesStart, name: &str) -> Option<String> {
  e.attributes()
    .flatten()
    .find(|a| a.key.local_name().as_ref() == name)
    .and_then(|a| normalized_owned(&a).ok())
}

pub fn find_attrs(e: &BytesStart, names: &[&str]) -> Vec<Option<String>> {
  let mut results = vec![None; names.len()];
  let mut remaining = names.len();

  for a in e.attributes().flatten() {
    if remaining == 0 {
      break;
    }

    let local = a.key.local_name();
    if let Some(slot) = names
      .iter()
      .position(|&n| local.as_ref() == n)
      .filter(|&idx| results[idx].is_none())
    {
      if let Ok(v) = normalized_owned(&a) {
        results[slot] = Some(v);
        remaining -= 1;
      }
    }
  }

  results
}
