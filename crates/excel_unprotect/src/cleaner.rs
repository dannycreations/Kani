use std::{
  collections::HashMap,
  fs::File,
  io::{Read, Write},
  path::Path,
  str,
};

use anyhow::Result;
use regex::Regex;
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::fs::{add_suffix, safe_save_path};

pub fn remove_protection_and_save(
  target_path: &Path,
  original_path: &Path,
) -> Result<()> {
  let file = File::open(target_path)?;

  let mut archive = match ZipArchive::new(file) {
    Ok(a) => a,
    Err(e) => {
      return Err(anyhow::anyhow!("Failed to open as Zip archive: {}", e));
    }
  };

  // Build sheet name map
  let mut sheet_map: HashMap<String, String> = HashMap::new();
  let mut rid_to_name: HashMap<String, String> = HashMap::new();

  // 1. Read workbook.xml to map r:id to sheet name
  if let Ok(mut wb_file) = archive.by_name("xl/workbook.xml") {
    let mut content = String::new();
    if wb_file.read_to_string(&mut content).is_ok() {
      let sheet_re =
        Regex::new(r#"<sheet\s+[^>]*name="([^"]+)"\s+[^>]*r:id="([^"]+)""#)
          .unwrap();
      // Fallback for different order or namespaces
      let sheet_re2 =
        Regex::new(r#"<sheet\s+[^>]*r:id="([^"]+)"\s+[^>]*name="([^"]+)""#)
          .unwrap();

      for cap in sheet_re.captures_iter(&content) {
        rid_to_name.insert(cap[2].to_string(), cap[1].to_string());
      }
      for cap in sheet_re2.captures_iter(&content) {
        rid_to_name.insert(cap[1].to_string(), cap[2].to_string());
      }
    }
  }

  // 2. Read workbook.xml.rels to map r:id to target filename
  if let Ok(mut rels_file) = archive.by_name("xl/_rels/workbook.xml.rels") {
    let mut content = String::new();
    if rels_file.read_to_string(&mut content).is_ok() {
      let rel_re = Regex::new(
        r#"<Relationship\s+[^>]*Id="([^"]+)"\s+[^>]*Target="([^"]+)""#,
      )
      .unwrap();
      for cap in rel_re.captures_iter(&content) {
        let rid = &cap[1];
        let target = &cap[2];
        if let Some(name) = rid_to_name.get(rid) {
          // Target might be relative, e.g., "worksheets/sheet1.xml" or "/xl/worksheets/sheet1.xml"
          // We need to match it with how ZipArchive lists files.
          // ZipArchive usually has "xl/worksheets/sheet1.xml".
          // Target in rels is usually relative to xl/ folder, so "worksheets/sheet1.xml".
          // So full path is "xl/" + target.

          let full_path = if target.starts_with('/') {
            target.trim_start_matches('/').to_string()
          } else {
            format!("xl/{}", target)
          };
          sheet_map.insert(full_path, name.clone());
        }
      }
    }
  }

  let clean_path = add_suffix(original_path, "_clean");
  let final_path = safe_save_path(&clean_path);
  let out_file = File::create(&final_path)?;
  let mut zip_writer = ZipWriter::new(out_file);

  let sheet_prot_regex = Regex::new(r"<sheetProtection[^>]*/>")?;
  let workbook_prot_regex = Regex::new(r"<workbookProtection[^>]*/>")?;
  let file_sharing_regex = Regex::new(r"<fileSharing[^>]*/>")?;

  for i in 0..archive.len() {
    let mut file = archive.by_index(i)?;
    let name = file.name().to_string();

    let options = FileOptions::<()>::default()
      .compression_method(file.compression())
      .unix_permissions(file.unix_mode().unwrap_or(0o755));

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let mut modified = false;
    let mut new_content_str = String::new();

    if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
      if let Ok(text) = str::from_utf8(&content) {
        if sheet_prot_regex.is_match(text) {
          new_content_str = sheet_prot_regex.replace(text, "").to_string();
          modified = true;

          let display_name = sheet_map
            .get(&name)
            .map(|s| s.as_str())
            .unwrap_or(name.as_str());
          println!("[+] Sheet protection removed: {}", display_name);
        }
      }
    } else if name == "xl/workbook.xml" {
      if let Ok(text) = str::from_utf8(&content) {
        let mut temp_text = text.to_string();
        if workbook_prot_regex.is_match(&temp_text) {
          temp_text = workbook_prot_regex.replace(&temp_text, "").to_string();
          modified = true;
          println!("[+] Workbook protection removed");
        }
        if file_sharing_regex.is_match(&temp_text) {
          temp_text = file_sharing_regex.replace(&temp_text, "").to_string();
          modified = true;
          println!("[+] File sharing protection removed");
        }
        if modified {
          new_content_str = temp_text;
        }
      }
    }

    zip_writer.start_file(name, options)?;
    if modified {
      zip_writer.write_all(new_content_str.as_bytes())?;
    } else {
      zip_writer.write_all(&content)?;
    }
  }

  zip_writer.finish()?;
  println!("[+] Saved cleaned copy: {}", final_path.display());

  Ok(())
}
