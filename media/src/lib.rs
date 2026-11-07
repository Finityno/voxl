//! Embeddable media primitives that do not initialize a process runtime, own
//! an application event loop, or link the standalone server's x264 dependency.

pub mod codec;
pub mod av1;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(all(test,not(target_os = "windows")))]
#[path="windows/bitstream.rs"]
mod windows_bitstream;
use codec::VideoCodec;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(feature = "webrtc")]
pub mod webrtc;

/// A complete Annex B video access unit. Parameter sets precede keyframes so
/// transports can establish or restart decoding without a separate header API.
#[derive(Debug)]
pub struct EncodedFrame {
    pub codec: VideoCodec,
    pub annex_b: Vec<u8>,
    pub keyframe: bool,
    pub presentation_time_micros: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderPreference { PreferHardware, RequireHardware, Software }

/// Local encoder experiments. These are requests, not measured capability,
/// codec admission, or a promise that the native encoder supports the mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EncoderRateControl { #[default] RealTime, LowLatency }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EncoderProfile { #[default] Compatible, H264High, H264ConstrainedHigh }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncoderTuning { pub rate_control: EncoderRateControl, pub profile: EncoderProfile }
impl EncoderTuning {
    pub fn validate(self, config: &EncoderConfig) -> Result<(), MediaError> {
        config.validate()?;
        if config.codec == VideoCodec::Av1 { return Err(MediaError::UnsupportedCodec); }
        // Experimental mode/profile requests must not silently fall back to
        // software or another tuning. The ordinary constructor keeps its
        // existing hardware-first/CPU-fallback behavior.
        if self != Self::default() && config.preference != EncoderPreference::RequireHardware {
            return Err(MediaError::Configuration);
        }
        if config.codec != VideoCodec::H264 && self.profile != EncoderProfile::Compatible {
            return Err(MediaError::Configuration);
        }
        Ok(())
    }
}

/// How native acceleration was established. A required/disabled encoder
/// specification is deliberately distinguished from a successful status query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderAcceleration { QueriedHardware, QueriedSoftware, RequiredHardware, DisabledHardware }
impl EncoderAcceleration {
    pub fn using_hardware(self) -> bool { matches!(self,Self::QueriedHardware|Self::RequiredHardware) }
    #[cfg(any(target_os = "macos",test))]
    pub(crate) fn without_status(hardware: bool, preference: EncoderPreference, tuning: EncoderTuning) -> Option<Self> {
        if !hardware && tuning == EncoderTuning::default() && matches!(preference,EncoderPreference::PreferHardware|EncoderPreference::Software) {
            Some(Self::DisabledHardware)
        } else if hardware && preference == EncoderPreference::RequireHardware && tuning.rate_control == EncoderRateControl::LowLatency {
            Some(Self::RequiredHardware)
        } else { None }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    pub codec: VideoCodec,
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
pub enum MediaError { Configuration, UnsupportedCodec, Platform { operation: &'static str, code: i32 }, Backpressure, InvalidFrame, Closed }
impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "desktop media: {self:?}") }
}
impl std::error::Error for MediaError {}

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Parse length-prefixed NAL units without trusting the native sample's length
/// fields. Kept platform-independent so malformed boundaries are tested on Linux.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn nal_units_to_annex_b(data: &[u8], length_bytes: usize, codec: VideoCodec) -> Result<(Vec<u8>, bool), MediaError> {
    if codec == VideoCodec::Av1 { return Err(MediaError::UnsupportedCodec); }
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
        match codec {
            VideoCodec::H264 => { keyframe |= nal[0] & 0x1f == 5; }
            VideoCodec::Hevc => {
                // The implemented HEVC Main path is single-layer. Reject a
                // truncated header, forbidden bit or zero temporal ID rather
                // than submitting malformed or multilayer data to a receiver.
                if nal.len() < 2 || nal[0] & 0x81 != 0 || nal[1] & 0xf8 != 0 || nal[1] & 7 == 0 {
                    return Err(MediaError::InvalidFrame);
                }
                keyframe |= (16..=21).contains(&((nal[0] >> 1) & 0x3f));
            }
            VideoCodec::Av1 => return Err(MediaError::UnsupportedCodec),
        }
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
    fn encoder_tuning_preserves_default_fallback_and_requires_explicit_hardware_experiments() {
        let mut config = EncoderConfig { codec: VideoCodec::H264, width: 1280, height: 720, fps: 30,
            bitrate_kbps: 2500, preference: EncoderPreference::PreferHardware };
        for preference in [EncoderPreference::PreferHardware,EncoderPreference::RequireHardware,EncoderPreference::Software] {
            config.preference = preference;
            assert!(EncoderTuning::default().validate(&config).is_ok());
            for rate_control in [EncoderRateControl::RealTime,EncoderRateControl::LowLatency] {
                for profile in [EncoderProfile::Compatible,EncoderProfile::H264High,EncoderProfile::H264ConstrainedHigh] {
                    let tuning = EncoderTuning { rate_control, profile };
                    assert_eq!(tuning.validate(&config).is_ok(),tuning == EncoderTuning::default() || preference == EncoderPreference::RequireHardware);
                }
            }
        }
        config.codec = VideoCodec::Hevc;config.preference = EncoderPreference::RequireHardware;
        assert!(EncoderTuning { rate_control: EncoderRateControl::LowLatency, profile: EncoderProfile::Compatible }.validate(&config).is_ok());
        assert_eq!(EncoderTuning { rate_control: EncoderRateControl::RealTime, profile: EncoderProfile::H264High }.validate(&config),Err(MediaError::Configuration));
        config.codec = VideoCodec::Av1;
        assert_eq!(EncoderTuning::default().validate(&config),Err(MediaError::UnsupportedCodec));
        config.codec = VideoCodec::H264;config.width = 1;
        assert_eq!(EncoderTuning::default().validate(&config),Err(MediaError::Configuration));
    }
    #[test]
    fn encoder_tuning_acceleration_observation_retains_strict_query_guards() {
        let standard = EncoderTuning::default();
        let low = EncoderTuning { rate_control: EncoderRateControl::LowLatency, ..standard };
        assert_eq!(EncoderAcceleration::without_status(false,EncoderPreference::Software,standard),Some(EncoderAcceleration::DisabledHardware));
        assert_eq!(EncoderAcceleration::without_status(false,EncoderPreference::PreferHardware,standard),Some(EncoderAcceleration::DisabledHardware));
        assert_eq!(EncoderAcceleration::without_status(true,EncoderPreference::RequireHardware,low),Some(EncoderAcceleration::RequiredHardware));
        for (hardware,preference,tuning) in [(true,EncoderPreference::RequireHardware,standard),(true,EncoderPreference::PreferHardware,standard),
            (true,EncoderPreference::PreferHardware,low),(false,EncoderPreference::RequireHardware,standard),(false,EncoderPreference::Software,low)] {
            assert_eq!(EncoderAcceleration::without_status(hardware,preference,tuning),None);
        }
        assert!(EncoderAcceleration::RequiredHardware.using_hardware());assert!(EncoderAcceleration::QueriedHardware.using_hardware());
        assert!(!EncoderAcceleration::DisabledHardware.using_hardware());assert!(!EncoderAcceleration::QueriedSoftware.using_hardware());
    }
    #[test]
    fn avcc_rejects_truncation_and_preserves_nal_boundaries() {
        let (output, key) = nal_units_to_annex_b(&[0,0,0,2,0x65,7,0,0,0,1,0x41],4,VideoCodec::H264).unwrap();
        assert!(key);
        assert_eq!(output, [0,0,0,1,0x65,7,0,0,0,1,0x41]);
        for input in [&[0,0,0,2,0x65][..], &[0,0,0,0], &[0xff,0xff,0xff,0xff], &[]] {
            assert!(nal_units_to_annex_b(input,4,VideoCodec::H264).is_err());
        }
        assert!(nal_units_to_annex_b(&[1,0x41],0,VideoCodec::H264).is_err());
        assert_eq!(nal_units_to_annex_b(&[1,0x41],1,VideoCodec::H264).unwrap(), (vec![0,0,0,1,0x41],false));
    }
    #[test]
    fn hevc_preserves_parameter_sets_and_recognizes_irap_without_accepting_bad_headers() {
        let data = [0,0,0,3,0x40,1,7,0,0,0,3,0x26,1,9];
        let (output,keyframe) = nal_units_to_annex_b(&data,4,VideoCodec::Hevc).unwrap();
        assert_eq!(output,[0,0,0,1,0x40,1,7,0,0,0,1,0x26,1,9]);
        assert!(keyframe);
        assert!(!nal_units_to_annex_b(&[2,2,1],1,VideoCodec::Hevc).unwrap().1);
        for data in [&[1,0x26][..],&[2,0x26,0],&[2,0xa6,1],&[2,0x27,1],&[2,0x26,9],&[3,0x26,1]] {
            assert_eq!(nal_units_to_annex_b(data,1,VideoCodec::Hevc).unwrap_err(),MediaError::InvalidFrame);
        }
        assert_eq!(nal_units_to_annex_b(&[2,0x26,1],1,VideoCodec::Av1).unwrap_err(),MediaError::UnsupportedCodec);
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

/// The actual HEVC Main profile/tier/level, read from the encoder's SPS.
/// Private fields prevent a caller from advertising another unimplemented
/// profile as Main. This does not establish the receiver's decoder capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HevcProfile {
    tier: bool,
    level: u8,
    compatibility: u32,
    constraints: [u8; 6],
}
impl HevcProfile {
    /// Whether an in-band replacement remains within this advertised Main
    /// profile. Keep tier, compatibility and constraints identical, and never
    /// raise the level. This is not receiver capability or authority evidence.
    pub fn permits_reconfiguration(&self,actual:&Self)->bool {
        self.tier==actual.tier && self.compatibility==actual.compatibility
            && self.constraints==actual.constraints && actual.level<=self.level
    }
    /// RFC 7798 format parameters for a single RTP stream, without decoding
    /// order numbers. Parameter sets are repeated in-band with keyframes.
    pub fn fmtp(&self) -> String {
        let constraints = self.constraints.iter().map(|value| format!("{value:02x}")).collect::<String>();
        format!("profile-space=0;profile-id=1;tier-flag={};level-id={};profile-compatibility-indicator={:08x};interop-constraints={constraints};tx-mode=SRST;sprop-max-don-diff=0",
            u8::from(self.tier),self.level,self.compatibility)
    }
}

/// Parse just the bounded general profile/tier/level prefix of a single-layer
/// HEVC SPS. Remove emulation-prevention bytes before reading fixed offsets;
/// zeros in compatibility flags otherwise move the apparent level byte.
pub fn hevc_main_profile(annex_b: &[u8]) -> Result<HevcProfile, MediaError> {
    if annex_b.len() > MAX_FRAME_BYTES { return Err(MediaError::InvalidFrame); }
    let offset = annex_b.windows(4).enumerate().find_map(|(offset,prefix)| {
        (prefix == [0,0,0,1] && annex_b.get(offset+4).is_some_and(|byte| (byte >> 1) & 0x3f == 33)).then_some(offset+4)
    }).ok_or(MediaError::InvalidFrame)?;
    let rest = &annex_b[offset..];
    let length = rest.windows(4).position(|prefix| prefix == [0,0,0,1]).unwrap_or(rest.len());
    let nal = &rest[..length];
    if nal.len() < 3 || nal[0] & 0x81 != 0 || nal[1] & 0xf8 != 0 || nal[1] & 7 == 0 { return Err(MediaError::InvalidFrame); }
    let mut bytes = nal[2..].iter().copied().peekable();
    let mut rbsp = [0; 13];
    let mut index = 0;
    let mut zeros = 0;
    while index < rbsp.len() {
        let byte = bytes.next().ok_or(MediaError::InvalidFrame)?;
        if zeros >= 2 && byte == 3 {
            if bytes.peek().is_none_or(|next| *next > 3) { return Err(MediaError::InvalidFrame); }
            zeros = 0;
            continue;
        }
        rbsp[index] = byte;
        index += 1;
        zeros = if byte == 0 { zeros+1 } else { 0 };
    }
    if rbsp[1] & 0xdf != 1 { return Err(MediaError::UnsupportedCodec); }
    if rbsp[12] == 0 { return Err(MediaError::InvalidFrame); }
    Ok(HevcProfile { tier: rbsp[1] & 0x20 != 0,level: rbsp[12],
        compatibility: u32::from_be_bytes(rbsp[2..6].try_into().map_err(|_| MediaError::InvalidFrame)?),
        constraints: rbsp[6..12].try_into().map_err(|_| MediaError::InvalidFrame)? })
}

#[cfg(test)]
mod hevc_profile_tests {
    use super::*;
    #[test]
    fn hevc_profile_reconfiguration_never_expands_advertised_constraints() {
        let original=HevcProfile {tier:false,level:123,compatibility:0x60000000,constraints:[0xb0,0,0,0,0,0]};
        assert!(original.permits_reconfiguration(&original));
        assert!(original.permits_reconfiguration(&HevcProfile {level:93,..original}));
        assert!(!original.permits_reconfiguration(&HevcProfile {level:150,..original}));
        assert!(!original.permits_reconfiguration(&HevcProfile {tier:true,..original}));
        assert!(!original.permits_reconfiguration(&HevcProfile {compatibility:0x40000000,..original}));
        assert!(!original.permits_reconfiguration(&HevcProfile {constraints:[0,0,0,0,0,0],..original}));
    }
    #[test]
    fn hevc_profile_removes_emulation_prevention_and_advertises_actual_level() {
        // SPS header, sps_max_sub_layers_minus1/temporal nesting, then Main
        // profile, compatibility flags, six constraint bytes and level 4.1.
        let sps = [0,0,0,1,0x42,1,1,1,0x60,0,0,3,0,0xb0,0,0,3,0,0,3,0,0x7b];
        let profile = hevc_main_profile(&sps).unwrap();
        assert_eq!(profile.fmtp(),"profile-space=0;profile-id=1;tier-flag=0;level-id=123;profile-compatibility-indicator=60000000;interop-constraints=b00000000000;tx-mode=SRST;sprop-max-don-diff=0");
        assert!(hevc_main_profile(&sps[..sps.len()-1]).is_err());
        let mut main10 = sps;
        main10[7] = 2;
        assert_eq!(hevc_main_profile(&main10).unwrap_err(),MediaError::UnsupportedCodec);
    }
}
