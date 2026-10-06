use gpui_kit::AssetSource;

use super::{EmbedAssets, ICON_BYTES};

#[test]
fn every_registered_icon_is_loadable() {
  let assets = EmbedAssets;
  for (icon, bytes) in ICON_BYTES {
    let loaded = assets.load(icon.path()).unwrap();
    assert!(
      loaded.is_some_and(|loaded| loaded == *bytes),
      "{} is registered without loadable bytes",
      icon.path()
    );
  }
}
