pub mod assets;
pub mod queue;

/// Default EBU R128 / ITU-R BS.1770 audio normalization settings optimized for YouTube playback.
///
/// Parameters:
/// - `I=-14`: Integrated Loudness target of -14 LUFS (Loudness Units Full Scale), matching YouTube's default loudness standard.
/// - `LRA=11`: Loudness Range target of 11 LU, preserving natural dynamic range without excessive compression for spoken audio/music.
/// - `TP=-1`: Maximum True Peak target of -1.0 dBTP to prevent inter-sample clipping during lossy audio re-encoding (e.g. AAC/Opus).
pub const DEFAULT_LOUDNORM_CONFIG: &str = "loudnorm=I=-14:LRA=11:TP=-1";

/// Default FFmpeg video and audio encoding flags optimized for NVENC YouTube uploads.
pub const DEFAULT_CUSTOM_VFLAGS: &[&str] = &[
  // --- Video Codec & NVENC Quality Profile ---
  // Use NVIDIA NVENC H.264 hardware encoder.
  "-c:v",
  "h264_nvenc",
  // H.264 High Profile for high quality HD video encoding.
  "-profile:v",
  "high",
  // Preset p7 (Highest Quality encoding preset in NVENC SDK).
  "-preset",
  "p7",
  // High Quality tuning mode for NVENC.
  "-tune",
  "hq",
  // --- Rate Control & Bitrate Allocation ---
  // Variable Bitrate (VBR) rate control mode.
  "-rc",
  "vbr",
  // Target average video bitrate set to 15 Mbps (recommended for 1080p60 YouTube uploads).
  "-b:v",
  "15M",
  // Maximum bitrate limit set to 20 Mbps for high-complexity motion scenes.
  "-maxrate",
  "20M",
  // VBR buffer size set to 40 Mb (2x maxrate) for VBR rate control smoothing.
  "-bufsize",
  "40M",
  // --- GOP (Group of Pictures) & Frame Structure ---
  // Keyframe interval (GOP size) set to 120 frames (2 seconds at 60 FPS, YouTube recommended).
  "-g",
  "120",
  // Maximum consecutive B-frames set to 2 (optimal for NVENC H.264 compression).
  "-bf",
  "2",
  // Lookahead window of 32 frames for optimal B-frame placement and bitrate distribution.
  "-rc-lookahead",
  "32",
  // --- Adaptive Quantization (Quality Fine-Tuning) ---
  // Enable spatial adaptive quantization to optimize bitrate allocation based on scene detail.
  "-spatial_aq",
  "1",
  // Enable temporal adaptive quantization to reduce motion artifacts across consecutive frames.
  "-temporal_aq",
  "1",
  // AQ strength set to 8 (balanced AQ scale 1-15 for high visual quality).
  "-aq-strength",
  "8",
  // --- Color Space & Pixel Format Specification ---
  // Pixel format set to 8-bit YUV 4:2:0 planarity for universal player and YouTube compatibility.
  "-pix_fmt",
  "yuv420p",
  // ITU-R BT.709 color space matrix.
  "-colorspace",
  "bt709",
  // ITU-R BT.709 color primaries.
  "-color_primaries",
  "bt709",
  // ITU-R BT.709 transfer characteristics (gamma curve).
  "-color_trc",
  "bt709",
  // Broadcast TV color range (MPEG range 16-235).
  "-color_range",
  "tv",
  // --- Audio Codec & Parameters ---
  // Audio codec set to AAC (Advanced Audio Coding).
  "-c:a",
  "aac",
  // Audio bitrate set to 384 kbps (high audio quality for YouTube stereo uploads).
  "-b:a",
  "384k",
  // Audio sampling rate set to 48 kHz (standard production audio sample rate).
  "-ar",
  "48000",
  // --- Others ---
  // Enable faststart by moving the MP4 moov atom to the beginning of the file for fast YouTube playback processing.
  "-movflags",
  "+faststart",
  // Set frame rate to 60 FPS.
  "-r",
  "60",
];
