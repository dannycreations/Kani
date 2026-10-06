#[derive(Debug, Clone)]
pub struct TrackDef {
  pub name: &'static str,
  pub index: usize,
  pub default_offset: f32,
}

#[derive(Debug, Clone)]
pub struct Preset {
  pub name: &'static str,
  pub tracks: &'static [TrackDef],
}

static BUILTINS: &[Preset] = &[Preset {
  name: "3-Track Recording (Mic / Discord / Game)",
  tracks: &[
    TrackDef {
      name: "Mic",
      index: 1,
      default_offset: -2.0,
    },
    TrackDef {
      name: "Discord",
      index: 2,
      default_offset: -6.0,
    },
    TrackDef {
      name: "Game",
      index: 0,
      default_offset: -16.0,
    },
  ],
}];

impl Preset {
  pub fn builtins() -> &'static [Preset] {
    BUILTINS
  }
}
