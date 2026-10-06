use std::sync::Arc;

use super::{AudioRenderer, TrackStats};
use crate::ffmpeg::settings::{AudioSettings, TrackConfig};

fn three_track_settings(
  mic_offset: f32,
  discord_offset: f32,
  game_offset: f32,
) -> AudioSettings {
  AudioSettings {
    single_track: false,
    tracks: vec![
      TrackConfig {
        name: Arc::from("Mic"),
        index: 1,
        offset: mic_offset,
      },
      TrackConfig {
        name: Arc::from("Discord"),
        index: 2,
        offset: discord_offset,
      },
      TrackConfig {
        name: Arc::from("Game"),
        index: 0,
        offset: game_offset,
      },
    ],
  }
}

#[test]
fn test_compute_mix_hierarchy() {
  let settings = three_track_settings(-2.0, -6.0, -16.0);

  // Helper closure: builds track stats in preset order
  // (Mic=index 0, Discord=index 1, Game=index 2) and computes volumes.
  let compute = |mm: f32, dm: f32, gm: f32| {
    let track_stats = vec![
      TrackStats {
        mean: Some(mm),
        ..Default::default()
      },
      TrackStats {
        mean: Some(dm),
        ..Default::default()
      },
      TrackStats {
        mean: Some(gm),
        ..Default::default()
      },
    ];

    let computed =
      AudioRenderer::compute_mix_volumes(&settings, &track_stats).unwrap();
    (computed[0], computed[1], computed[2]) // (mic_vol, discord_vol, game_vol)
  };

  // Case 1: Normal levels
  let (mic_vol, discord_vol, game_vol) = compute(-20.0, -25.0, -30.0);
  assert_eq!(mic_vol, -2.0);
  assert_eq!(discord_vol, -1.0);
  assert_eq!(game_vol, -6.0);

  // Case 2: Quiet tracks (needs boosting, but still active)
  let (mic_vol, discord_vol, game_vol) = compute(-20.0, -35.0, -40.0);
  assert_eq!(mic_vol, -2.0);
  assert_eq!(discord_vol, 9.0);
  assert_eq!(game_vol, 4.0);

  // Case 3: Extreme quiet/silence (hitting the boost limit)
  let (mic_vol, discord_vol, game_vol) = compute(-20.0, -90.0, -30.0);
  // Discord at -90.0 (< -45.0) is muted: gets default offset -6.0.
  // The hierarchy reference level is preserved at -26.0.
  assert_eq!(mic_vol, -2.0);
  assert_eq!(discord_vol, -6.0);
  assert_eq!(game_vol, -6.0);

  // Case 4: Extreme loud (checking clamp to -100)
  let (mic_vol, discord_vol, game_vol) = compute(-50.0, 0.0, 0.0);
  // Mic at -50.0 (< -45.0) is muted. Reference = -20.0 + (-2.0) = -22.0.
  // Discord at 0.0 (active). Target = -22 - 4 = -26.0. Vol = -26.0.
  // Game at 0.0 (active). Target = -26 - 10 = -36.0. Vol = -36.0.
  assert_eq!(mic_vol, -2.0);
  assert_eq!(discord_vol, -26.0);
  assert_eq!(game_vol, -36.0);

  // Case 5: Mic muted, Discord/Game active
  let (mic_vol, discord_vol, game_vol) = compute(-90.0, -25.0, -30.0);
  // mm = -90.0 (muted). mic_ref_post = -22.0.
  // dm = -25.0 (active). discord_target = -26.0. discord_vol = -1.0.
  // gm = -30.0 (active). game_target = -36.0. game_vol = -6.0.
  assert_eq!(mic_vol, -2.0);
  assert_eq!(discord_vol, -1.0);
  assert_eq!(game_vol, -6.0);
}

#[test]
fn test_append_filter_args() {
  let tracks = three_track_settings(-2.0, -6.0, -16.0).tracks;

  let mut mixed = Vec::new();
  AudioRenderer::append_filter_args(
    &mut mixed,
    &tracks,
    Some(&[-2.0, -1.0, -6.0]),
    "loudnorm=I=-14",
  );
  assert_eq!(
    mixed,
    [
      "-filter_complex",
      "[0:a:1]volume=-2.0dB[mic];[0:a:2]volume=-1.0dB[discord];[0:a:0]volume=-6.0dB[game];[mic][discord][game]amix=inputs=3:weights='1 1 1':dropout_transition=2:normalize=0[mixed];[mixed]loudnorm=I=-14[out]",
      "-map",
      "0:v:0",
      "-map",
      "[out]",
    ]
  );

  let mut loudnorm_only = Vec::new();
  AudioRenderer::append_filter_args(
    &mut loudnorm_only,
    &tracks,
    None,
    "loudnorm=I=-14",
  );
  assert_eq!(loudnorm_only, ["-af", "loudnorm=I=-14"]);
}
