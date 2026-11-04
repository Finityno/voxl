//! Embeddable media primitives that do not initialize a process runtime, own
//! an application event loop, or link the standalone server's x264 dependency.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(feature = "webrtc")]
pub mod webrtc;

/// A complete Annex B H.264 access unit. Parameter sets precede every IDR so
/// transports can establish or restart decoding without a separate header API.
#[derive(Debug)]
pub struct EncodedFrame {
    pub annex_b: Vec<u8>,
    pub keyframe: bool,
    pub presentation_time_micros: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderPreference { PreferHardware, RequireHardware, Software }

#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub preference: EncoderPreference,
}

impl EncoderConfig {
    pub fn validate(&self) -> Result<(), MediaError> {
        if self.width < 320 || self.height < 180 || self.width > 4096 || self.height > 4096
            || !self.width.is_multiple_of(2) || !self.height.is_multiple_of(2)
            || u64::from(self.width) * u64::from(self.height) > 3840 * 2160
            || !(1..=60).contains(&self.fps) || !(250..=20_000).contains(&self.bitrate_kbps)
        { return Err(MediaError::Configuration); }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaError { Configuration, Platform(i32), Backpressure, InvalidFrame, Closed }
impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "desktop media: {self:?}") }
}
impl std::error::Error for MediaError {}

#[cfg(any(target_os = "macos", feature = "webrtc", test))]
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Parse length-prefixed NAL units without trusting the native sample's length
/// fields. Kept platform-independent so malformed boundaries are tested on Linux.
#[cfg(any(target_os = "macos", test))]
fn avcc_to_annex_b(data: &[u8], length_bytes: usize) -> Result<(Vec<u8>, bool), MediaError> {
    if !(1..=4).contains(&length_bytes) || data.is_empty() || data.len() > MAX_FRAME_BYTES {
        return Err(MediaError::InvalidFrame);
    }
    let mut out = Vec::with_capacity(data.len().saturating_add(1024).min(MAX_FRAME_BYTES));
    let mut offset = 0;
    let mut keyframe = false;
    while offset < data.len() {
        let prefix = data.get(offset..offset + length_bytes).ok_or(MediaError::InvalidFrame)?;
        let length = prefix.iter().fold(0usize, |value, byte| (value << 8) | usize::from(*byte));
        offset += length_bytes;
        if length == 0 || length > data.len() - offset || out.len().saturating_add(4 + length) > MAX_FRAME_BYTES {
            return Err(MediaError::InvalidFrame);
        }
        let nal = &data[offset..offset + length];
        keyframe |= nal[0] & 0x1f == 5;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
        offset += length;
    }
    Ok((out, keyframe))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avcc_rejects_truncation_and_preserves_nal_boundaries() {
        let (output, key) = avcc_to_annex_b(&[0,0,0,2,0x65,7,0,0,0,1,0x41],4).unwrap();
        assert!(key);
        assert_eq!(output, [0,0,0,1,0x65,7,0,0,0,1,0x41]);
        for input in [&[0,0,0,2,0x65][..], &[0,0,0,0], &[0xff,0xff,0xff,0xff], &[]] {
            assert!(avcc_to_annex_b(input,4).is_err());
        }
        assert!(avcc_to_annex_b(&[1,0x41],0).is_err());
        assert_eq!(avcc_to_annex_b(&[1,0x41],1).unwrap(), (vec![0,0,0,1,0x41],false));
    }
}

/// Read the advertised H.264 profile/constraint/level bytes from a complete
/// Annex B keyframe. Negotiate the encoder's actual level, not a fixed 720p
/// profile for a 1080p or 4K stream.
pub fn h264_profile_level_id(annex_b: &[u8]) -> Result<[u8; 3], MediaError> {
    for (offset, prefix) in annex_b.windows(4).enumerate() {
        if prefix == [0,0,0,1] && annex_b.get(offset+4).is_some_and(|nal| nal & 0x1f == 7) {
            return annex_b.get(offset+5..offset+8).and_then(|bytes| bytes.try_into().ok()).ok_or(MediaError::InvalidFrame);
        }
    }
    Err(MediaError::InvalidFrame)
}
