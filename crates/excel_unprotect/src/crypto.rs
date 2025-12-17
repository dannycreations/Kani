use std::{
  io::{Cursor, Read},
  str,
};

use aes::cipher::{generic_array::GenericArray, BlockDecryptMut, KeyIvInit};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use byteorder::{LittleEndian, ReadBytesExt};
use cbc::Decryptor;
use cfb::CompoundFile;
use quick_xml::{events::Event, reader::Reader};
use sha2::{Digest, Sha512};

type Aes256CbcDec = Decryptor<aes::Aes256>;

pub fn is_ole_file(buffer: &[u8]) -> bool {
  buffer.len() > 8 && &buffer[0..8] == b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1"
}

pub fn decrypt_office_file(data: &[u8], password: &str) -> Result<Vec<u8>> {
  let cursor = Cursor::new(data);
  let mut cfb = CompoundFile::open(cursor)?;

  // 1. Read EncryptionInfo
  let mut enc_info_data = Vec::new();
  {
    let mut enc_info_stream = cfb.open_stream("/EncryptionInfo")?;
    enc_info_stream.read_to_end(&mut enc_info_data)?;
  }

  // Parse Version
  let mut cursor = Cursor::new(&enc_info_data);
  let v_major = cursor.read_u16::<LittleEndian>()?;
  let v_minor = cursor.read_u16::<LittleEndian>()?;

  if v_major != 4 || v_minor != 4 {
    return Err(anyhow!("Unsupported encryption version: {}.{}. Only Agile Encryption (4.4) is supported.", v_major, v_minor));
  }

  let _flags = cursor.read_u32::<LittleEndian>()?;

  let xml_offset = cursor.position() as usize;
  let xml_data = &enc_info_data[xml_offset..];
  let xml_str = str::from_utf8(xml_data)?;

  // Parse XML using quick-xml for robustness
  let mut reader = Reader::from_str(xml_str);
  reader.config_mut().trim_text(true);

  let mut key_data_salt = None;
  let mut enc_key_salt = None;
  let mut spin_count = None;
  let mut key_bits = None;
  let mut alg_id = None;
  let mut enc_key_value = None;

  let mut buf = Vec::new();

  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
        let name = e.name();
        let name_str = std::str::from_utf8(name.as_ref())?;

        // Handle namespaced tags like p:encryptedKey or just encryptedKey
        let local_name = if let Some(idx) = name_str.find(':') {
          &name_str[idx + 1..]
        } else {
          name_str
        };

        if local_name == "keyData" {
          for attr in e.attributes() {
            let attr = attr?;
            if attr.key.as_ref() == b"saltValue" {
              key_data_salt = Some(attr.unescape_value()?.into_owned());
            }
          }
        } else if local_name == "encryptedKey" {
          for attr in e.attributes() {
            let attr = attr?;
            match attr.key.as_ref() {
              b"saltValue" => {
                enc_key_salt = Some(attr.unescape_value()?.into_owned())
              }
              b"spinCount" => {
                spin_count = Some(attr.unescape_value()?.into_owned())
              }
              b"keyBits" => {
                key_bits = Some(attr.unescape_value()?.into_owned())
              }
              b"hashAlgorithm" => {
                alg_id = Some(attr.unescape_value()?.into_owned())
              }
              b"encryptedKeyValue" => {
                enc_key_value = Some(attr.unescape_value()?.into_owned())
              }
              _ => (),
            }
          }
        }
      }
      Ok(Event::Eof) => break,
      Err(e) => {
        return Err(anyhow!(
          "Error parsing XML at position {}: {:?}",
          reader.buffer_position(),
          e
        ))
      }
      _ => (),
    }
    buf.clear();
  }

  let pkg_salt_b64 =
    key_data_salt.ok_or(anyhow!("Missing keyData saltValue"))?;
  let enc_key_salt_b64 =
    enc_key_salt.ok_or(anyhow!("Missing encryptedKey saltValue"))?;
  let spin_count_str = spin_count.ok_or(anyhow!("Missing spinCount"))?;
  let key_bits_str = key_bits.ok_or(anyhow!("Missing keyBits"))?;
  let alg_str = alg_id.ok_or(anyhow!("Missing hashAlgorithm"))?;
  let enc_key_val_b64 =
    enc_key_value.ok_or(anyhow!("Missing encryptedKeyValue"))?;

  let salt = STANDARD.decode(&enc_key_salt_b64)?;
  let spin_count: u32 = spin_count_str.parse()?;
  let key_bits: u32 = key_bits_str.parse()?;
  let encrypted_key_value = STANDARD.decode(&enc_key_val_b64)?;

  if alg_str != "SHA512" {
    return Err(anyhow!(
      "Unsupported hash algorithm: {}. Only SHA512 is currently implemented.",
      alg_str
    ));
  }

  let mut hasher = Sha512::new();
  hasher.update(&salt);
  let pw_utf16: Vec<u8> = password
    .encode_utf16()
    .flat_map(|c| c.to_le_bytes())
    .collect();
  hasher.update(&pw_utf16);
  let mut h_n = hasher.finalize();

  for i in 0..spin_count {
    let mut iterator = [0u8; 4];
    iterator.copy_from_slice(&i.to_le_bytes());

    let mut hasher = Sha512::new();
    hasher.update(iterator);
    hasher.update(h_n);
    h_n = hasher.finalize();
  }

  let block_key = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];

  let mut hasher = Sha512::new();
  hasher.update(h_n);
  hasher.update(block_key);
  let kek_hash = hasher.finalize();

  let key_len = (key_bits / 8) as usize;
  let kek = &kek_hash[0..key_len];

  // IV for KEK is derived from the KEK salt, clipped to 16 bytes (or padded)
  let mut iv = [0u8; 16];
  if salt.len() >= 16 {
    iv.copy_from_slice(&salt[0..16]);
  } else {
    iv[0..salt.len()].copy_from_slice(&salt);
  }

  let mut key_decrypter = Aes256CbcDec::new(kek.into(), &iv.into());
  let mut content_key = encrypted_key_value.clone();

  if content_key.len() % 16 != 0 {
    return Err(anyhow!("Encrypted Key Value size not multiple of 16"));
  }

  let mut blocks: Vec<GenericArray<u8, _>> = content_key
    .chunks_exact(16)
    .map(|b| *GenericArray::from_slice(b))
    .collect();

  // Use decrypt_blocks_mut to avoid padding errors for now, we can inspect result
  key_decrypter.decrypt_blocks_mut(&mut blocks);

  // Copy back
  let mut i = 0;
  for block in blocks {
    content_key[i..i + 16].copy_from_slice(&block);
    i += 16;
  }

  // Now we need the salt from keyData for the package decryption
  let pkg_salt = STANDARD.decode(&pkg_salt_b64)?;

  let mut enc_pkg_data = Vec::new();
  {
    let mut enc_pkg_stream = cfb.open_stream("/EncryptedPackage")?;
    enc_pkg_stream.read_to_end(&mut enc_pkg_data)?;
  }

  let mut decrypted_pkg = Vec::new();

  let total_size = (&enc_pkg_data[0..8]).read_u64::<LittleEndian>()? as usize;
  let payload = &enc_pkg_data[8..];

  let chunk_size = 4096;
  let chunks = payload.chunks(chunk_size);

  for (i, chunk) in chunks.enumerate() {
    let mut iv_hasher = Sha512::new();
    iv_hasher.update(&pkg_salt);
    let block_idx = (i as u32).to_le_bytes();
    iv_hasher.update(block_idx);
    let iv_hash = iv_hasher.finalize();
    let iv = &iv_hash[0..16];

    let actual_key = &content_key[0..32]; // Taking first 32 bytes

    let mut decryptor = Aes256CbcDec::new(actual_key.into(), iv.into());

    if chunk.len() % 16 != 0 {
      return Err(anyhow!("Chunk size not multiple of 16"));
    }

    let chunk_buf = chunk.to_vec();

    let mut blocks: Vec<GenericArray<u8, _>> = chunk_buf
      .chunks_exact(16)
      .map(|b| *GenericArray::from_slice(b))
      .collect();

    decryptor.decrypt_blocks_mut(&mut blocks);

    for block in blocks {
      decrypted_pkg.extend_from_slice(&block);
    }
  }

  if decrypted_pkg.len() < total_size {
    return Err(anyhow!("Decrypted size mismatch"));
  }
  decrypted_pkg.truncate(total_size);

  Ok(decrypted_pkg)
}
