use std::{
  io::{Read, Seek, Write},
  str,
};

use aes::{
  cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit},
  Aes256,
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cfb::CompoundFile;
use quick_xml::{events::Event, reader::Reader};
use sha2::{Digest, Sha512};

const BLOCK_KEY: [u8; 8] = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];
const OLE_HEADER: &[u8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

pub fn is_ole_file(buffer: &[u8]) -> bool {
  buffer.len() >= 8 && &buffer[0..8] == OLE_HEADER
}

struct AgileEncryptionInfo {
  key_data_salt: Vec<u8>,
  enc_key_salt: Vec<u8>,
  spin_count: u32,
  key_bits: u32,
  encrypted_key_value: Vec<u8>,
}

impl AgileEncryptionInfo {
  fn from_xml(xml_str: &str) -> Result<Self> {
    let mut reader = Reader::from_str(xml_str);
    reader.config_mut().trim_text(true);

    let mut key_data_salt = None;
    let mut enc_key_salt = None;
    let mut spin_count = None;
    let mut key_bits = None;
    let mut enc_key_value = None;
    let mut alg_id = None;

    let mut buf = Vec::new();

    loop {
      match reader.read_event_into(&mut buf) {
        Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
          match e.local_name().as_ref() {
            b"keyData" => {
              for a in e.attributes().flatten() {
                if a.key.local_name().as_ref() == b"saltValue" {
                  key_data_salt = Some(a.unescape_value()?.into_owned());
                }
              }
            }
            b"encryptedKey" => {
              for a in e.attributes().flatten() {
                match a.key.local_name().as_ref() {
                  b"saltValue" => {
                    enc_key_salt = Some(a.unescape_value()?.into_owned())
                  }
                  b"spinCount" => {
                    spin_count = Some(a.unescape_value()?.into_owned())
                  }
                  b"keyBits" => {
                    key_bits = Some(a.unescape_value()?.into_owned())
                  }
                  b"hashAlgorithm" => {
                    alg_id = Some(a.unescape_value()?.into_owned())
                  }
                  b"encryptedKeyValue" => {
                    enc_key_value = Some(a.unescape_value()?.into_owned())
                  }
                  _ => {}
                }
              }
            }
            _ => {}
          }
        }
        Ok(Event::Eof) => break,
        Err(e) => return Err(anyhow!("XML error: {}", e)),
        _ => (),
      }
      buf.clear();
    }

    if alg_id.as_deref() != Some("SHA512") {
      return Err(anyhow!("Unsupported hash algorithm"));
    }

    Ok(Self {
      key_data_salt: STANDARD
        .decode(key_data_salt.context("Missing keyData salt")?)?,
      enc_key_salt: STANDARD
        .decode(enc_key_salt.context("Missing encryptedKey salt")?)?,
      spin_count: spin_count.context("Missing spinCount")?.parse()?,
      key_bits: key_bits.context("Missing keyBits")?.parse()?,
      encrypted_key_value: STANDARD
        .decode(enc_key_value.context("Missing encryptedKeyValue")?)?,
    })
  }

  fn derive_key(
    &self,
    password: &str,
  ) -> Result<(GenericArray<u8, aes::cipher::typenum::U32>, Vec<u8>)> {
    let mut hasher = Sha512::new();
    hasher.update(&self.enc_key_salt);
    for c in password.encode_utf16() {
      hasher.update(c.to_le_bytes());
    }
    let mut h_n = hasher.finalize();

    for i in 0..self.spin_count {
      let mut hasher = Sha512::new();
      hasher.update(i.to_le_bytes());
      hasher.update(h_n);
      h_n = hasher.finalize();
    }

    let mut hasher = Sha512::new();
    hasher.update(h_n);
    hasher.update(BLOCK_KEY);
    let kek_hash = hasher.finalize();

    let key_len = (self.key_bits / 8) as usize;
    let kek = &kek_hash[0..key_len];

    let mut iv = [0u8; 16];
    let salt_len = self.enc_key_salt.len().min(16);
    iv[..salt_len].copy_from_slice(&self.enc_key_salt[..salt_len]);

    let cipher = Aes256::new(GenericArray::from_slice(kek));
    let mut encrypted_key = self.encrypted_key_value.clone();

    if !encrypted_key.len().is_multiple_of(16) {
      return Err(anyhow!("Encrypted Key Value size not multiple of 16"));
    }

    let mut prev_block = *GenericArray::from_slice(&iv);

    for block in encrypted_key.chunks_mut(16) {
      let current_ciphertext = *GenericArray::from_slice(block);
      let mut state = current_ciphertext;

      cipher.decrypt_block(&mut state);

      for (r, v) in state.iter_mut().zip(prev_block.iter()) {
        *r ^= *v;
      }

      block.copy_from_slice(state.as_slice());
      prev_block = current_ciphertext;
    }

    let actual_key = &encrypted_key[0..32];
    Ok((
      GenericArray::clone_from_slice(actual_key),
      self.key_data_salt.clone(),
    ))
  }
}

pub fn decrypt_file<R, W>(
  reader: R,
  mut writer: W,
  password: &str,
) -> Result<()>
where
  R: Read + Seek,
  W: Write,
{
  let mut cfb = CompoundFile::open(reader)?;

  let mut enc_info_data = Vec::with_capacity(4096);
  cfb
    .open_stream("/EncryptionInfo")?
    .read_to_end(&mut enc_info_data)?;

  if enc_info_data.len() < 8 {
    return Err(anyhow!("EncryptionInfo stream too short"));
  }

  let v_major = u16::from_le_bytes([enc_info_data[0], enc_info_data[1]]);
  let v_minor = u16::from_le_bytes([enc_info_data[2], enc_info_data[3]]);

  if v_major != 4 || v_minor != 4 {
    return Err(anyhow!(
      "Unsupported encryption version: {}.{}",
      v_major,
      v_minor
    ));
  }

  let xml_str = str::from_utf8(&enc_info_data[8..])?;
  let info = AgileEncryptionInfo::from_xml(xml_str)?;
  let (content_key, pkg_salt) = info.derive_key(password)?;

  let mut enc_pkg_stream = cfb.open_stream("/EncryptedPackage")?;
  let mut size_buf = [0u8; 8];
  enc_pkg_stream.read_exact(&mut size_buf)?;
  let total_size = u64::from_le_bytes(size_buf) as usize;

  let mut buffer = vec![0u8; 64 * 1024];
  let mut block_idx = 0u32;
  let mut bytes_decrypted = 0;

  let mut base_iv_hasher = Sha512::new();
  base_iv_hasher.update(&pkg_salt);

  let cipher = Aes256::new(&content_key);

  loop {
    let mut pos = 0;
    while pos < buffer.len() {
      let n = enc_pkg_stream.read(&mut buffer[pos..])?;
      if n == 0 {
        break;
      }
      pos += n;
    }
    if pos == 0 {
      break;
    }

    let chunk = &mut buffer[..pos];
    let remainder = chunk.len() % 16;
    if remainder != 0 && bytes_decrypted + chunk.len() < total_size {
      return Err(anyhow!("Chunk size {} not multiple of 16", chunk.len()));
    }

    for segment in chunk.chunks_mut(4096) {
      let mut iv_hasher = base_iv_hasher.clone();
      iv_hasher.update(block_idx.to_le_bytes());
      let iv_hash = iv_hasher.finalize();

      let mut prev_block = *GenericArray::from_slice(&iv_hash[0..16]);

      for block in segment.chunks_mut(16) {
        if block.len() < 16 {
          break;
        }

        let current_ciphertext = *GenericArray::from_slice(block);
        let mut state = current_ciphertext;

        cipher.decrypt_block(&mut state);

        for (r, v) in state.iter_mut().zip(prev_block.iter()) {
          *r ^= *v;
        }

        block.copy_from_slice(state.as_slice());
        prev_block = current_ciphertext;
      }
      block_idx += 1;
    }

    let to_write = if bytes_decrypted + pos > total_size {
      &chunk[..(total_size - bytes_decrypted)]
    } else {
      chunk
    };

    writer.write_all(to_write)?;
    bytes_decrypted += to_write.len();
  }

  if bytes_decrypted < total_size {
    return Err(anyhow!(
      "Decrypted size mismatch: expected {}, got {}",
      total_size,
      bytes_decrypted
    ));
  }

  Ok(())
}
