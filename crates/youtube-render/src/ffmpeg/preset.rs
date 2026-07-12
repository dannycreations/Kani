use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackDef {
  pub name: Arc<str>,
  pub index: usize,
  pub default_offset: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
  pub name: Arc<str>,
  pub tracks: Vec<TrackDef>,
}

static BUILTINS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
  vec![Preset {
    name: Arc::from("3-Track Recording (Mic / Discord / Game)"),
    tracks: vec![
      TrackDef {
        name: Arc::from("Mic"),
        index: 1,
        default_offset: -2.0,
      },
      TrackDef {
        name: Arc::from("Discord"),
        index: 2,
        default_offset: -6.0,
      },
      TrackDef {
        name: Arc::from("Game"),
        index: 0,
        default_offset: -16.0,
      },
    ],
  }]
});

impl Preset {
  pub fn builtins() -> &'static [Preset] {
    &BUILTINS
  }

  pub fn default_index() -> usize {
    0
  }
}
