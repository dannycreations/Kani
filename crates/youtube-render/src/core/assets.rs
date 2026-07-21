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

  pub fn all() -> &'static [Self] {
    &[
      Self::Settings,
      Self::ChevronUp,
      Self::ArrowUp,
      Self::ArrowDown,
      Self::Delete,
      Self::ExternalLink,
      Self::FolderOpen,
      Self::Check,
      Self::Plus,
      Self::Play,
      Self::Stop,
    ]
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
    let bytes = match path {
      "icons/settings.svg" => {
        include_bytes!("../../assets/icons/settings.svg").as_slice()
      }
      "icons/chevron-up.svg" => {
        include_bytes!("../../assets/icons/chevron-up.svg").as_slice()
      }
      "icons/arrow-up.svg" => {
        include_bytes!("../../assets/icons/arrow-up.svg").as_slice()
      }
      "icons/arrow-down.svg" => {
        include_bytes!("../../assets/icons/arrow-down.svg").as_slice()
      }
      "icons/plus.svg" => {
        include_bytes!("../../assets/icons/plus.svg").as_slice()
      }
      "icons/play.svg" => {
        include_bytes!("../../assets/icons/play.svg").as_slice()
      }
      "icons/stop.svg" => {
        include_bytes!("../../assets/icons/stop.svg").as_slice()
      }
      "icons/delete.svg" => {
        include_bytes!("../../assets/icons/delete.svg").as_slice()
      }
      "icons/external-link.svg" => {
        include_bytes!("../../assets/icons/external-link.svg").as_slice()
      }
      "icons/folder-open.svg" => {
        include_bytes!("../../assets/icons/folder-open.svg").as_slice()
      }
      "icons/check.svg" => {
        include_bytes!("../../assets/icons/check.svg").as_slice()
      }
      _ => return Ok(None),
    };
    Ok(Some(Cow::Borrowed(bytes)))
  }

  fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
    Ok(
      IconName::all()
        .iter()
        .map(|icon| SharedString::from(icon.path()))
        .collect(),
    )
  }
}
