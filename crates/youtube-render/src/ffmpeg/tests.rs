use std::{
  process::Command,
  sync::{Arc, Mutex},
};

use crate::ffmpeg::{
  ini::IniDocument,
  process::{kill_all_children, register_child, ACTIVE_CHILDREN},
  progress::{FfmpegParser, ProgressInfo, VolumeDetectInfo, VolumeType},
  settings::{AudioSettings, TrackConfig},
  track::{AudioRenderer, TrackStats},
};

#[test]
fn test_active_children_registration() {
  #[cfg(windows)]
  let mut cmd = Command::new("cmd");
  #[cfg(not(windows))]
  let mut cmd = Command::new("sh");

  #[cfg(windows)]
  cmd.args(["/C", "ping 127.0.0.1 -n 2"]);
  #[cfg(not(windows))]
  cmd.args(["-c", "sleep 1"]);

  if let Ok(child) = cmd.spawn() {
    let handle = Arc::new(Mutex::new(Some(child)));
    register_child(Arc::clone(&handle));

    {
      let lock = ACTIVE_CHILDREN.lock().unwrap();
      assert!(lock.iter().any(|h| Arc::ptr_eq(h, &handle)));
    }

    kill_all_children();

    {
      let lock = ACTIVE_CHILDREN.lock().unwrap();
      assert!(lock.is_empty());
    }

    let child_lock = handle.lock().unwrap();
    assert!(child_lock.is_none());
  }
}

#[test]
fn test_parse_duration() {
  assert_eq!(
    FfmpegParser::parse_duration("  Duration: 00:01:23.45, start: 0.000000"),
    Some(83.45)
  );
  assert_eq!(
    FfmpegParser::parse_duration("Duration: 02:00:00.00"),
    Some(7200.0)
  );
  assert_eq!(
    FfmpegParser::parse_duration("random line with no duration"),
    None
  );
}

#[test]
fn test_parse_volume_detect() {
  assert_eq!(
    FfmpegParser::parse_volume_detect(
      "[Parsed_volumedetect_0 @ 0x123] mean_volume: -24.3 dB"
    ),
    Some((0, true, -24.3))
  );
  assert_eq!(
    FfmpegParser::parse_volume_detect(
      "[Parsed_volumedetect_2 @ 0x456] max_volume: -0.1 dB"
    ),
    Some((2, false, -0.1))
  );
  assert_eq!(
    FfmpegParser::parse_volume_detect(
      "[Parsed_someotherfilter] mean_volume: -24.3 dB"
    ),
    None
  );

  assert_eq!(
    FfmpegParser::parse_volume_detect_typed(
      "[Parsed_volumedetect_0 @ 0x123] mean_volume: -24.3 dB"
    ),
    Some(VolumeDetectInfo {
      track_index: 0,
      volume_type: VolumeType::Mean,
      volume_db: -24.3,
    })
  );
}

#[test]
fn test_extract_loudnorm_val() {
  assert_eq!(
    FfmpegParser::extract_loudnorm_val(
      "[Parsed_loudnorm_0 @ 0x55d3e0] \t\"input_i\" : \"-14.84\",",
      "\"input_i\""
    ),
    Some(-14.84)
  );
  assert_eq!(
    FfmpegParser::extract_loudnorm_val(
      "  \"input_lra\" : \"4.40\",",
      "\"input_lra\""
    ),
    Some(4.40)
  );
  assert_eq!(
    FfmpegParser::extract_loudnorm_val(
      "\"target_offset\" : \"0.84\"",
      "\"target_offset\""
    ),
    Some(0.84)
  );
  assert_eq!(
    FfmpegParser::extract_loudnorm_val(
      "[Parsed_loudnorm_0] \"input_tp\" : \"-2.00\"",
      "\"input_tp\""
    ),
    Some(-2.00)
  );
  assert_eq!(
    FfmpegParser::extract_loudnorm_val("random line", "\"input_i\""),
    None
  );
}

#[test]
fn test_progress_info_parsing() {
  let mut info = ProgressInfo::new();

  // First block
  assert!(!info.parse_line("frame=150"));
  assert!(!info.parse_line("out_time_us=5000000"));
  assert!(!info.parse_line("out_time=00:00:05.000000"));
  assert!(!info.parse_line("speed= 310x"));
  assert!(info.parse_line("progress=continue"));

  assert_eq!(info.out_time_us, Some(5000000));
  assert_eq!(info.out_time.as_deref(), Some("00:00:05"));
  assert_eq!(info.speed.as_deref(), Some("310x"));

  // Second block updates values
  assert!(!info.parse_line("out_time_us=10000000"));
  assert!(!info.parse_line("out_time=00:00:10.000000"));
  assert!(!info.parse_line("speed= 250x"));
  assert!(info.parse_line("progress=continue"));

  assert_eq!(info.out_time_us, Some(10000000));
  assert_eq!(info.out_time.as_deref(), Some("00:00:10"));
  assert_eq!(info.speed.as_deref(), Some("250x"));

  // Third block with some missing keys preserves previous values or handles missing ones correctly
  assert!(!info.parse_line("out_time_us=15000000"));
  assert!(info.parse_line("progress=end"));
  assert_eq!(info.out_time_us, Some(15000000));
  assert_eq!(info.out_time.as_deref(), Some("00:00:10"));
}

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

#[test]
fn test_ini_parsing_general() {
  let ini = "\
[section1]
key1 = val1
key2 = val2

[section2]
key3 = val3
";
  let doc = IniDocument::parse(ini);
  assert_eq!(doc.get("section1", "key1"), Some("val1"));
  assert_eq!(doc.get("section1", "key2"), Some("val2"));
  assert_eq!(doc.get("section2", "key3"), Some("val3"));
  assert_eq!(doc.get("section2", "key4"), None);
}
