mod process;
mod progress;
mod settings;
mod track;

pub use process::{kill_all_children, RenderProcess};
pub use progress::{JobProgress, StepType};
pub use settings::{AudioSettings, RenderSettings};
pub use track::AudioTrack;

#[cfg(test)]
mod tests {
  use std::{
    process::Command,
    sync::{Arc, Mutex},
  };

  use super::{
    process::{kill_all_children, register_child, ACTIVE_CHILDREN},
    progress::{
      extract_loudnorm_val, parse_duration, parse_volume_detect, ProgressInfo,
    },
    track::{compute_mix_volumes, AudioTrack, TrackStats},
    AudioSettings,
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
      parse_duration("  Duration: 00:01:23.45, start: 0.000000"),
      Some(83.45)
    );
    assert_eq!(parse_duration("Duration: 02:00:00.00"), Some(7200.0));
    assert_eq!(parse_duration("random line with no duration"), None);
  }

  #[test]
  fn test_parse_volume_detect() {
    assert_eq!(
      parse_volume_detect(
        "[Parsed_volumedetect_0 @ 0x123] mean_volume: -24.3 dB"
      ),
      Some((0, true, -24.3))
    );
    assert_eq!(
      parse_volume_detect(
        "[Parsed_volumedetect_2 @ 0x456] max_volume: -0.1 dB"
      ),
      Some((2, false, -0.1))
    );
    assert_eq!(
      parse_volume_detect("[Parsed_someotherfilter] mean_volume: -24.3 dB"),
      None
    );
  }

  #[test]
  fn test_extract_loudnorm_val() {
    assert_eq!(
      extract_loudnorm_val(
        "[Parsed_loudnorm_0 @ 0x55d3e0] \t\"input_i\" : \"-14.84\",",
        "\"input_i\""
      ),
      Some(-14.84)
    );
    assert_eq!(
      extract_loudnorm_val("  \"input_lra\" : \"4.40\",", "\"input_lra\""),
      Some(4.40)
    );
    assert_eq!(
      extract_loudnorm_val("\"target_offset\" : \"0.84\"", "\"target_offset\""),
      Some(0.84)
    );
    assert_eq!(
      extract_loudnorm_val(
        "[Parsed_loudnorm_0] \"input_tp\" : \"-2.00\"",
        "\"input_tp\""
      ),
      Some(-2.00)
    );
    assert_eq!(extract_loudnorm_val("random line", "\"input_i\""), None);
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
    assert_eq!(info.out_time.as_deref(), Some("00:00:05.000000"));
    assert_eq!(info.speed.as_deref(), Some("310x"));

    // Second block updates values
    assert!(!info.parse_line("out_time_us=10000000"));
    assert!(!info.parse_line("out_time=00:00:10.000000"));
    assert!(!info.parse_line("speed= 250x"));
    assert!(info.parse_line("progress=continue"));

    assert_eq!(info.out_time_us, Some(10000000));
    assert_eq!(info.out_time.as_deref(), Some("00:00:10.000000"));
    assert_eq!(info.speed.as_deref(), Some("250x"));

    // Third block with some missing keys preserves previous values or handles missing ones correctly
    assert!(!info.parse_line("out_time_us=15000000"));
    assert!(info.parse_line("progress=end"));
    assert_eq!(info.out_time_us, Some(15000000));
    assert_eq!(info.out_time.as_deref(), Some("00:00:10.000000"));
  }

  #[test]
  fn test_compute_mix_hierarchy() {
    let settings = AudioSettings {
      single_track: false,
      game_offset: -16.0,
      mic_offset: -2.0,
      discord_offset: -6.0,
    };

    // Helper closure to compute volumes matching the new mute guard logic
    let compute = |gm: f32, mm: f32, dm: f32| {
      let mut tracks = vec![TrackStats::default(); 3];
      tracks[AudioTrack::Game.index()].mean = Some(gm);
      tracks[AudioTrack::Mic.index()].mean = Some(mm);
      tracks[AudioTrack::Discord.index()].mean = Some(dm);

      let computed = compute_mix_volumes(&settings, &tracks).unwrap();
      (
        computed[&AudioTrack::Mic],
        computed[&AudioTrack::Discord],
        computed[&AudioTrack::Game],
      )
    };

    // Case 1: Normal levels
    let (mic_vol, discord_vol, game_vol) = compute(-30.0, -20.0, -25.0);
    assert_eq!(mic_vol, -2.0);
    assert_eq!(discord_vol, -1.0);
    assert_eq!(game_vol, -6.0);

    // Case 2: Quiet tracks (needs boosting, but still active)
    let (mic_vol, discord_vol, game_vol) = compute(-40.0, -20.0, -35.0);
    assert_eq!(mic_vol, -2.0);
    assert_eq!(discord_vol, 9.0);
    assert_eq!(game_vol, 4.0);

    // Case 3: Extreme quiet/silence (hitting the boost limit)
    let (mic_vol, discord_vol, game_vol) = compute(-30.0, -20.0, -90.0);
    // Since Discord is at -90.0 (< -45.0), it is detected as muted.
    // It gets default offset -6.0 (instead of +30.0 to avoid boosting silence).
    // The hierarchy reference level is preserved at -26.0.
    assert_eq!(mic_vol, -2.0);
    assert_eq!(discord_vol, -6.0);
    assert_eq!(game_vol, -6.0); // game target is discord_ref_post - 10 = -36.0, gm is -30.0 -> -6.0

    // Case 4: Extreme loud (checking clamp to -100)
    let (mic_vol, discord_vol, game_vol) = compute(0.0, -50.0, 0.0);
    // Since Mic is at -50.0 (< -45.0), it is detected as muted.
    // Reference level falls back to -20.0 + mic_vol = -22.0.
    // Discord is at 0.0 (active). Target: -22 - 4 = -26.0. Vol = clamp(-26.0 - 0.0) = -26.0.
    // discord_ref_post = -26.0.
    // Game is at 0.0 (active). Target: -26 - 10 = -36.0. Vol = clamp(-36.0 - 0.0) = -36.0.
    assert_eq!(mic_vol, -2.0);
    assert_eq!(discord_vol, -26.0);
    assert_eq!(game_vol, -36.0);

    // Case 5: Mic muted, Discord/Game active
    let (mic_vol, discord_vol, game_vol) = compute(-30.0, -90.0, -25.0);
    // mm = -90.0 (muted). mic_ref_post = -22.0.
    // dm = -25.0 (active). discord_target = -26.0. discord_vol = -1.0. discord_ref_post = -26.0.
    // gm = -30.0 (active). game_target = -36.0. game_vol = -6.0.
    assert_eq!(mic_vol, -2.0);
    assert_eq!(discord_vol, -1.0);
    assert_eq!(game_vol, -6.0);
  }
}
