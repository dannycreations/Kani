use std::sync::Arc;

use super::{AudioSettings, TrackConfig};

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
fn test_ini_round_trip() {
  let original = three_track_settings(-2.0, -6.0, -16.0);
  let ini = original.to_ini();
  let parsed = AudioSettings::from_ini(&ini).unwrap();
  assert_eq!(original, parsed);
}

#[test]
fn test_ini_round_trip_single_track() {
  let original = AudioSettings {
    single_track: true,
    tracks: vec![],
  };
  let ini = original.to_ini();
  let parsed = AudioSettings::from_ini(&ini).unwrap();
  assert_eq!(original, parsed);
}

#[test]
fn test_ini_with_comments_and_whitespace() {
  let ini = "\
; this is a comment
# another comment

[audio]
single_track = false

[track.0]
name = Vocal
index = 0
offset = -3.5

";
  let parsed = AudioSettings::from_ini(ini).unwrap();
  assert!(!parsed.single_track);
  assert_eq!(parsed.tracks.len(), 1);
  assert_eq!(&*parsed.tracks[0].name, "Vocal");
  assert_eq!(parsed.tracks[0].index, 0);
  assert!((parsed.tracks[0].offset - (-3.5)).abs() < 0.001);
}

#[test]
fn test_ini_missing_track_key_fails() {
  let ini = "\
[audio]
single_track = false

[track.0]
name = Mic
index = 1
";
  // Missing 'offset' key
  let result = AudioSettings::from_ini(ini);
  assert!(result.is_err());
}
