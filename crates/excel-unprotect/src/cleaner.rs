use std::{
  collections::HashMap,
  fs::File,
  io::{copy, BufWriter, Read, Seek, Write},
  path::Path,
};

use anyhow::{anyhow, Result};
use quick_xml::{
  events::{BytesStart, Event},
  reader::Reader,
  writer::Writer,
};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::{
  fs::{add_suffix, safe_save_path},
  xml::find_attrs,
};

const XML_BUFFER_CAPACITY: usize = 8192;

struct WorkbookMap {
  sheet_map: HashMap<String, String>,
  workbook_xml: Option<Vec<u8>>,
}

fn read_all(mut reader: impl Read, size_hint: u64) -> std::io::Result<Vec<u8>> {
  let mut buf = Vec::with_capacity(size_hint as usize);
  reader.read_to_end(&mut buf)?;
  Ok(buf)
}

fn read_zip_entry<R: Read + Seek>(
  archive: &mut ZipArchive<R>,
  name: &str,
) -> Option<Vec<u8>> {
  let mut file = archive.by_name(name).ok()?;
  let size = file.size();
  read_all(&mut file, size).ok()
}

fn for_each_element(
  data: &[u8],
  target: &str,
  mut on_match: impl FnMut(&BytesStart),
) {
  let mut reader = Reader::from_reader(data);
  reader.config_mut().trim_text(true);
  let mut buf = Vec::new();

  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
        if e.local_name().as_ref() == target {
          on_match(e);
        }
      }
      Ok(Event::Eof) | Err(_) => break,
      _ => {}
    }
    buf.clear();
  }
}

fn needs_protection_removal(content: &[u8]) -> bool {
  const PROTECTION: &[u8] = b"Protection";
  const FILE_SHARING: &[u8] = b"fileSharing";

  let n = content.len();
  for i in 0..n {
    let b = content[i];
    if b == PROTECTION[0] && content[i..].starts_with(PROTECTION) {
      return true;
    }
    if b == FILE_SHARING[0] && content[i..].starts_with(FILE_SHARING) {
      return true;
    }
  }
  false
}

impl WorkbookMap {
  fn new<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Self {
    let mut rid_to_name = HashMap::new();
    let mut sheet_map = HashMap::new();

    let workbook_xml = read_zip_entry(archive, "xl/workbook.xml");
    if let Some(content) = workbook_xml.as_deref() {
      for_each_element(content, "sheet", |e| {
        let mut vals = find_attrs(e, &["name", "id"]);
        if let (Some(name), Some(rid)) = (vals[0].take(), vals[1].take()) {
          rid_to_name.insert(rid, name);
        }
      });
    }

    if let Some(rels) = read_zip_entry(archive, "xl/_rels/workbook.xml.rels") {
      for_each_element(&rels, "Relationship", |e| {
        let mut vals = find_attrs(e, &["Id", "Target"]);
        if let (Some(id), Some(target)) = (vals[0].take(), vals[1].take()) {
          if let Some(name) = rid_to_name.get(&id) {
            let path = match target.strip_prefix('/') {
              Some(stripped) => stripped.to_string(),
              None => format!("xl/{target}"),
            };
            sheet_map.insert(path, name.clone());
          }
        }
      });
    }

    Self {
      sheet_map,
      workbook_xml,
    }
  }

  fn get_sheet_name(&self, path: &str) -> Option<&str> {
    self.sheet_map.get(path).map(String::as_str)
  }
}

fn is_vba_file(name: &str) -> bool {
  name.contains("vbaProject.bin")
    || name.contains("vbaProjectSignature.bin")
    || name.contains("macrosheets")
    || name.ends_with(".xlsm")
}

pub fn remove_protection_and_save<R: Read + Seek>(
  source: R,
  original_path: &Path,
  disable_macros: bool,
) -> Result<()> {
  let mut archive =
    ZipArchive::new(source).map_err(|e| anyhow!("Failed to open Zip: {e}"))?;
  let mut wb_map = WorkbookMap::new(&mut archive);

  let clean_path = add_suffix(original_path, "_clean");
  let final_path = safe_save_path(&clean_path);
  let out_file = File::create(&final_path)?;
  let mut zip_writer = ZipWriter::new(BufWriter::new(out_file));

  let mut xml_buf = Vec::with_capacity(XML_BUFFER_CAPACITY);

  for i in 0..archive.len() {
    let mut file = archive.by_index(i)?;
    let name = file.name().to_string();

    if is_vba_file(&name) {
      if disable_macros {
        println!("[+] Removed macro/VBA file: {name}");
        continue;
      } else {
        println!("[!] WARNING: Macro/VBA file preserved: {name}");
      }
    }

    let options = FileOptions::<()>::default()
      .compression_method(file.compression())
      .unix_permissions(file.unix_mode().unwrap_or(0o755));

    zip_writer.start_file(&name, options)?;

    let is_worksheet =
      name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml");
    let is_workbook = name == "xl/workbook.xml";

    if is_worksheet || is_workbook {
      let content = match (is_workbook, wb_map.workbook_xml.take()) {
        (true, Some(cached)) => cached,
        _ => {
          let size = file.size();
          read_all(&mut file, size)?
        }
      };

      if needs_protection_removal(&content) {
        let mut reader = Reader::from_reader(content.as_slice());
        let mut writer = Writer::new(&mut zip_writer);

        loop {
          match reader.read_event_into(&mut xml_buf) {
            Ok(ev @ (Event::Start(_) | Event::Empty(_))) => {
              let keep = match &ev {
                Event::Start(e) | Event::Empty(e) => {
                  !should_remove(e, is_worksheet, &name, &wb_map)
                }
                _ => unreachable!(),
              };
              if keep {
                writer.write_event(ev)?;
              }
            }
            Ok(Event::Eof) => break,
            Ok(e) => {
              writer.write_event(e)?;
            }
            Err(e) => return Err(anyhow!("XML parsing error: {e}")),
          }
          xml_buf.clear();
        }
      } else {
        zip_writer.write_all(&content)?;
      }
    } else {
      copy(&mut file, &mut zip_writer)?;
    }
  }

  zip_writer.finish()?;
  println!("[+] Saved cleaned copy: {}", final_path.display());

  Ok(())
}

fn should_remove(
  e: &BytesStart,
  is_worksheet: bool,
  name: &str,
  wb_map: &WorkbookMap,
) -> bool {
  match e.local_name().as_ref() {
    "sheetProtection" if is_worksheet => {
      let display_name = wb_map.get_sheet_name(name).unwrap_or(name);
      println!("[+] Sheet protection removed: {display_name}");
      true
    }
    "workbookProtection" => {
      println!("[+] Workbook protection removed");
      true
    }
    "fileSharing" => {
      println!("[+] File sharing protection removed");
      true
    }
    _ => false,
  }
}
