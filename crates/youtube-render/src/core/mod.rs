/// Default EBU R128 / ITU-R BS.1770 audio normalization settings optimized for YouTube playback.
///
/// Parameters:
/// - `I=-14`: Integrated Loudness target of -14 LUFS (Loudness Units Full Scale), matching YouTube's default loudness standard.
/// - `LRA=11`: Loudness Range target of 11 LU, preserving natural dynamic range without excessive compression for spoken audio/music.
/// - `TP=-1`: Maximum True Peak target of -1.0 dBTP to prevent inter-sample clipping during lossy audio re-encoding (e.g. AAC/Opus).
pub const DEFAULT_LOUDNORM_CONFIG: &str = "loudnorm=I=-14:LRA=11:TP=-1";
