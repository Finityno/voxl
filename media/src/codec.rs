//! Codec policy is independent of transport and native backend. Capabilities
//! must describe the exact requested geometry, fps, profile and chroma format;
//! a family name alone is not proof a device can sustain that stream.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec { Av1, Hevc, H264 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceleration {
    Hardware,
    /// A measured software path meeting the requested real-time budget.
    SoftwareRealtime,
    Unavailable,
    /// Not probed, or only file playback/codec-family support is known.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecCapability {
    pub codec: VideoCodec,
    pub encode: Acceleration,
    pub decode: Acceleration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoftwareFallback { Never, DecodeOnly, EncodeAndDecode }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecSelection {
    pub codec: VideoCodec,
    pub encode: Acceleration,
    pub decode: Acceleration,
}

/// Prefer a hardware encode/decode pair, following the supplied codec order.
/// Software is considered only when no such pair exists and the caller allows
/// it. Unknown capability never silently turns into an expensive CPU path.
/// This policy does not advertise codecs an endpoint has not implemented.
pub fn select_codec(preferences: &[VideoCodec], host: &[CodecCapability], viewer: &[CodecCapability], fallback: SoftwareFallback) -> Option<CodecSelection> {
    for software in [false,true] {
        for codec in preferences {
            let Some(encode) = host.iter().find(|capability| capability.codec == *codec).map(|capability| capability.encode) else { continue; };
            let Some(decode) = viewer.iter().find(|capability| capability.codec == *codec).map(|capability| capability.decode) else { continue; };
            let encode_ok = encode == Acceleration::Hardware || (software && fallback == SoftwareFallback::EncodeAndDecode && encode == Acceleration::SoftwareRealtime);
            let decode_ok = decode == Acceleration::Hardware || (software && fallback != SoftwareFallback::Never && decode == Acceleration::SoftwareRealtime);
            if encode_ok && decode_ok { return Some(CodecSelection {codec: *codec,encode,decode}); }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codec_selection_uses_both_endpoints_and_measured_acceleration() {
        use Acceleration::*;
        use VideoCodec::*;
        let preferences = [Av1,Hevc,H264];
        let capability = |codec,encode,decode| CodecCapability {codec,encode,decode};
        let mut host = vec![capability(Av1,Unavailable,Hardware),capability(Hevc,Hardware,Hardware),capability(H264,Hardware,Hardware)];
        let mut viewer = vec![capability(Av1,Unavailable,Hardware),capability(Hevc,Unavailable,Hardware),capability(H264,Unavailable,Hardware)];
        assert_eq!(select_codec(&preferences,&host,&viewer,SoftwareFallback::EncodeAndDecode).unwrap().codec,Hevc);
        host[0].encode = Hardware;
        assert_eq!(select_codec(&preferences,&host,&viewer,SoftwareFallback::Never).unwrap().codec,Av1);
        assert_eq!(select_codec(&preferences,&host[2..],&viewer[2..],SoftwareFallback::Never).unwrap().codec,H264);
        host[0].encode = SoftwareRealtime;
        viewer[1].decode = Unknown;
        assert_eq!(select_codec(&preferences,&host,&viewer,SoftwareFallback::EncodeAndDecode).unwrap().codec,H264);
        host[2].encode = Unavailable;
        assert!(select_codec(&preferences,&host,&viewer,SoftwareFallback::Never).is_none());
        assert_eq!(select_codec(&preferences,&host,&viewer,SoftwareFallback::EncodeAndDecode).unwrap().codec,Av1);
        viewer[0].decode = Unknown;
        assert!(select_codec(&preferences,&host,&viewer,SoftwareFallback::EncodeAndDecode).is_none());
    }
}
