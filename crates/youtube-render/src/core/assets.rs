use std::borrow::Cow;

use anyhow::Result;
use gpui_kit::{component::IconNamed, AssetSource, SharedString};

#[cfg(test)]
#[path = "assets_test.rs"]
mod assets_test;

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

static ICON_BYTES: &[(IconName, &[u8])] = &[
  (
    IconName::Settings,
    include_bytes!("../../assets/icons/settings.svg").as_slice(),
  ),
  (
    IconName::ChevronUp,
    include_bytes!("../../assets/icons/chevron-up.svg").as_slice(),
  ),
  (
    IconName::ArrowUp,
    include_bytes!("../../assets/icons/arrow-up.svg").as_slice(),
  ),
  (
    IconName::ArrowDown,
    include_bytes!("../../assets/icons/arrow-down.svg").as_slice(),
  ),
  (
    IconName::Delete,
    include_bytes!("../../assets/icons/delete.svg").as_slice(),
  ),
  (
    IconName::ExternalLink,
    include_bytes!("../../assets/icons/external-link.svg").as_slice(),
  ),
  (
    IconName::FolderOpen,
    include_bytes!("../../assets/icons/folder-open.svg").as_slice(),
  ),
  (
    IconName::Check,
    include_bytes!("../../assets/icons/check.svg").as_slice(),
  ),
  (
    IconName::Plus,
    include_bytes!("../../assets/icons/plus.svg").as_slice(),
  ),
  (
    IconName::Play,
    include_bytes!("../../assets/icons/play.svg").as_slice(),
  ),
  (
    IconName::Stop,
    include_bytes!("../../assets/icons/stop.svg").as_slice(),
  ),
];

pub struct EmbedAssets;

impl AssetSource for EmbedAssets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    Ok(
      ICON_BYTES
        .iter()
        .find(|(icon, _)| icon.path() == path)
        .map(|(_, bytes)| Cow::Borrowed(*bytes)),
    )
  }

  fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
    Ok(
      ICON_BYTES
        .iter()
        .map(|(icon, _)| SharedString::from(icon.path()))
        .collect(),
    )
  }
}
