use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use gpui_component::IconNamed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
  Settings,
  ChevronUp,
  ArrowUp,
  ArrowDown,
  Delete,
  ExternalLink,
  FolderOpen,
  Check,
  Plus,
  Play,
  Stop,
}

impl IconName {
  pub fn path(self) -> &'static str {
    match self {
      Self::Settings => "icons/settings.svg",
      Self::ChevronUp => "icons/chevron-up.svg",
      Self::ArrowUp => "icons/arrow-up.svg",
      Self::ArrowDown => "icons/arrow-down.svg",
      Self::Delete => "icons/delete.svg",
      Self::ExternalLink => "icons/external-link.svg",
      Self::FolderOpen => "icons/folder-open.svg",
      Self::Check => "icons/check.svg",
      Self::Plus => "icons/plus.svg",
      Self::Play => "icons/play.svg",
      Self::Stop => "icons/stop.svg",
    }
  }
}

impl IconNamed for IconName {
  fn path(self) -> SharedString {
    SharedString::from(self.path())
  }
}

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
      "icons/plus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/plus.svg"
      )))),
      "icons/play.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/play.svg"
      )))),
      "icons/stop.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/stop.svg"
      )))),
      "icons/delete.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/delete.svg"
      )))),
      "icons/external-link.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/external-link.svg"
      )))),
      "icons/folder-open.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/folder-open.svg"
      )))),
      "icons/check.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
        "../assets/icons/check.svg"
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
      "icons/plus.svg".into(),
      "icons/play.svg".into(),
      "icons/stop.svg".into(),
      "icons/delete.svg".into(),
      "icons/external-link.svg".into(),
      "icons/folder-open.svg".into(),
      "icons/check.svg".into(),
    ])
  }
}
