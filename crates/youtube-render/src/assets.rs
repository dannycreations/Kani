use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

pub struct EmbedAssets;

impl AssetSource for EmbedAssets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    match path {
      "icons/settings.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/settings.svg"
      )))),
      "icons/chevron-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/chevron-up.svg"
      )))),
      "icons/arrow-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/arrow-up.svg"
      )))),
      "icons/arrow-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/arrow-down.svg"
      )))),
      "icons/delete.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/delete.svg"
      )))),
      _ => Ok(None),
    }
  }

  fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
    Ok(vec![
      "icons/settings.svg".into(),
      "icons/chevron-up.svg".into(),
      "icons/arrow-up.svg".into(),
      "icons/arrow-down.svg".into(),
      "icons/delete.svg".into(),
    ])
  }
}
