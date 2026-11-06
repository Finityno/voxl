//! Explicit padding-only writes without changing ordinary RTP serialization.
use shared::{error::{Error,Result},marshal::{Marshal,MarshalSize}};

/// Interceptor attribute for a local padding-only packet. The entire zero
/// payload becomes authenticated RFC 3550 padding before SRTP encryption.
/// Serialized size stays unchanged through congestion control and pacing.
#[derive(Debug,Clone,Copy)]
pub struct RtpPaddingOnly;
impl RtpPaddingOnly {
    /// Serialize validated zero bytes as padding, preserving packet length.
    /// Rejects ordinary payload, markers, a missing padding flag or >255 octets.
    pub fn marshal(packet:&rtp::Packet)->Result<bytes::Bytes> {
        if !packet.header.padding || packet.header.marker || packet.payload.iter().any(|byte|*byte!=0) {return Err(Error::Other("invalid padding-only RTP".into()));}
        let padding=packet.payload.len().checked_add(4-packet.payload.len()%4).filter(|value|*value<=255).ok_or_else(||Error::Other("excessive padding-only RTP".into()))?;
        let mut serialized=vec![0;packet.marshal_size()];packet.marshal_to(&mut serialized)?;
        let last=serialized.last_mut().ok_or_else(||Error::Other("empty padding-only RTP".into()))?;*last=padding as u8;
        Ok(serialized.into())
    }
}
