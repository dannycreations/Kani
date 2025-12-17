use std::{
  collections::HashMap,
  fs::File,
  io::{self, BufReader, BufWriter, Read, Seek},
  path::Path,
};

use anyhow::{anyhow, Result};
use quick_xml::{
  events::{BytesStart, Event},
  reader::Reader,
  writer::Writer,
};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::fs::{add_suffix, safe_save_path};

struct WorkbookMap {
  sheet_map: HashMap<String, String>,
}

impl WorkbookMap {
  fn new<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Self {
    let mut rid_to_name = HashMap::new();
    let mut sheet_map = HashMap::new();
    let mut xml_buf = Vec::new();

    if let Ok(wb) = archive.by_name("xl/workbook.xml") {
      let mut reader = Reader::from_reader(BufReader::new(wb));
      reader.config_mut().trim_text(true);

      loop {
        match reader.read_event_into(&mut xml_buf) {
          Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
            if e.local_name().as_ref() == b"sheet" =>
          {
            if let (Some(name), Some(rid)) =
              (get_attr(e, b"name"), get_attr(e, b"id"))
            {
              rid_to_name.insert(rid, name);
            }
          }
          Ok(Event::Eof) => break,
          _ => {}
        }
        xml_buf.clear();
      }
    }

    if let Ok(rels) = archive.by_name("xl/_rels/workbook.xml.rels") {
      let mut reader = Reader::from_reader(BufReader::new(rels));
      reader.config_mut().trim_text(true);

      loop {
        match reader.read_event_into(&mut xml_buf) {
          Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
            if e.local_name().as_ref() == b"Relationship" =>
          {
            if let (Some(id), Some(target)) =
              (get_attr(e, b"Id"), get_attr(e, b"Target"))
            {
              if let Some(name) = rid_to_name.get(&id) {
                let path = if target.starts_with('/') {
                  target.trim_start_matches('/').to_string()
                } else {
                  format!("xl/{}", target)
                };
                sheet_map.insert(path, name.clone());
              }
            }
          }
          Ok(Event::Eof) => break,
          _ => {}
        }
        xml_buf.clear();
      }
    }

    Self { sheet_map }
  }

  fn get_sheet_name(&self, path: &str) -> Option<&str> {
    self.sheet_map.get(path).map(|s| s.as_str())
  }
}

fn get_attr(e: &BytesStart, name: &[u8]) -> Option<String> {
  e.attributes()
    .flatten()
    .find(|a| a.key.local_name().as_ref() == name)
    .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

pub fn remove_protection_and_save(
  target_path: &Path,
  original_path: &Path,
  disable_macros: bool,
) -> Result<()> {
  let file = File::open(target_path)?;
  let reader = BufReader::new(file);

  let mut archive = ZipArchive::new(reader)
    .map_err(|e| anyhow!("Failed to open Zip: {}", e))?;
  let wb_map = WorkbookMap::new(&mut archive);

  let clean_path = add_suffix(original_path, "_clean");
  let final_path = safe_save_path(&clean_path);
  let out_file = File::create(&final_path)?;
  let mut zip_writer = ZipWriter::new(BufWriter::new(out_file));

  let mut xml_buf = Vec::new();

  for i in 0..archive.len() {
    let mut file = archive.by_index(i)?;
    let name = file.name().to_string();

    let is_macro_file = name.contains("vbaProject.bin")
      || name.contains("vbaProjectSignature.bin");

    if is_macro_file {
      if disable_macros {
        println!("[+] Removed macro/VBA file: {}", name);
        continue;
      } else {
        println!("[!] WARNING: Macro/VBA file preserved: {}", name);
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
      let mut reader = Reader::from_reader(BufReader::new(file));
      let mut writer = Writer::new(&mut zip_writer);

      loop {
        match reader.read_event_into(&mut xml_buf) {
          Ok(Event::Start(ref e)) => {
            if !should_remove(e, is_worksheet, &name, &wb_map) {
              writer.write_event(Event::Start(e.clone()))?;
            }
          }
          Ok(Event::Empty(ref e)) => {
            if !should_remove(e, is_worksheet, &name, &wb_map) {
              writer.write_event(Event::Empty(e.clone()))?;
            }
          }
          Ok(Event::End(ref e)) => {
            writer.write_event(Event::End(e.clone()))?;
          }
          Ok(Event::Eof) => break,
          Ok(e) => {
            writer.write_event(e)?;
          }
          Err(e) => return Err(anyhow!("XML parsing error: {}", e)),
        }
        xml_buf.clear();
      }
    } else {
      io::copy(&mut file, &mut zip_writer)?;
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
    b"sheetProtection" if is_worksheet => {
      let display_name = wb_map.get_sheet_name(name).unwrap_or(name);
      println!("[+] Sheet protection removed: {}", display_name);
      true
    }
    b"workbookProtection" => {
      println!("[+] Workbook protection removed");
      true
    }
    b"fileSharing" => {
      println!("[+] File sharing protection removed");
      true
    }
    _ => false,
  }
}
