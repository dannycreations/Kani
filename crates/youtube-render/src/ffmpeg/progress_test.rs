use super::{FfmpegParser, ProgressInfo, VolumeDetectInfo, VolumeType};

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
    Some(VolumeDetectInfo {
      track_index: 0,
      volume_type: VolumeType::Mean,
      volume_db: -24.3,
    })
  );
  assert_eq!(
    FfmpegParser::parse_volume_detect(
      "[Parsed_volumedetect_2 @ 0x456] max_volume: -0.1 dB"
    ),
    Some(VolumeDetectInfo {
      track_index: 2,
      volume_type: VolumeType::Max,
      volume_db: -0.1,
    })
  );
  assert_eq!(
    FfmpegParser::parse_volume_detect(
      "[Parsed_someotherfilter] mean_volume: -24.3 dB"
    ),
    None
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
  let mut info = ProgressInfo::default();

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
