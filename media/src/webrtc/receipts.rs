//! Optional peer-reported decoded pictures. No grant, quality or source-equality
//! authority is inferred from a receipt, and no receipt permits frame skipping.
use super::{VideoFrameStamp,WebRtcError};
use std::{sync::Arc,time::{Duration,Instant}};
use webrtc::data_channel::{DataChannel,DataChannelEvent,RTCDataChannelInit};

pub const LABEL:&str="fincode-decoded-frames";
pub const PROTOCOL:&str="fincode-decoded-v1";
const MAGIC:[u8;4]=*b"FCD1";
const HISTORY:usize=64;
const MAX_AGE:Duration=Duration::from_secs(2);

/// A peer's claim that a recently written picture was decoded. The presentation
/// time is recovered from the sender's bounded ledger, never trusted from wire.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DecodedFrameReceipt {pub stamp:VideoFrameStamp,pub presentation_time_micros:i64}

pub(super) fn options()->RTCDataChannelInit {
    RTCDataChannelInit {ordered:false,max_retransmits:Some(0),protocol:PROTOCOL.into(),..Default::default()}
}
pub(super) fn decode(data:&[u8],is_string:bool)->Result<VideoFrameStamp,WebRtcError> {
    if is_string || data.len()!=12 || data[..4]!=MAGIC {return Err(WebRtcError::InvalidFrame);}
    let generation=u32::from_be_bytes(data[4..8].try_into().map_err(|_|WebRtcError::InvalidFrame)?);
    let rtp_timestamp=u32::from_be_bytes(data[8..12].try_into().map_err(|_|WebRtcError::InvalidFrame)?);
    Ok(VideoFrameStamp {generation:u64::from(generation),rtp_timestamp})
}

#[derive(Clone,Copy)]
struct Written {stamp:VideoFrameStamp,pts:i64,at:Instant,ambiguous:bool}
struct Ledger {rows:[Option<Written>;HISTORY],next:usize,accepted:Option<i64>}
impl Ledger {
    fn new()->Self {Self {rows:[None;HISTORY],next:0,accepted:None}}
    fn record(&mut self,stamp:VideoFrameStamp,pts:i64,now:Instant) {
        let mut ambiguous=false;
        for row in self.rows.iter_mut().flatten() {
            if row.stamp==stamp && now.checked_duration_since(row.at).is_some_and(|age|age<=MAX_AGE) {row.ambiguous=true;ambiguous=true;}
        }
        self.rows[self.next]=Some(Written {stamp,pts,at:now,ambiguous});self.next=(self.next+1)%HISTORY;
    }
    fn accept(&mut self,stamp:VideoFrameStamp,generation:u64,now:Instant)->Option<DecodedFrameReceipt> {
        if stamp.generation!=generation {return None;}
        let mut rows=self.rows.iter().flatten().filter(|row|row.stamp==stamp);
        let row=rows.next()?;
        if rows.next().is_some() || row.ambiguous || now.checked_duration_since(row.at).is_none_or(|age|age>MAX_AGE)
            || self.accepted.is_some_and(|previous|row.pts<=previous) {return None;}
        self.accepted=Some(row.pts);Some(DecodedFrameReceipt {stamp,presentation_time_micros:row.pts})
    }
}

enum ChannelPhase {Connecting,Open}
pub(super) struct Active {
    channel:Arc<dyn DataChannel>,phase:ChannelPhase,ledger:Ledger,
    window:Instant,messages:u16,
}
pub(super) enum Receipts {Disabled,Active(Box<Active>)}
pub(super) enum Outcome {None,Decoded(DecodedFrameReceipt),Closed}
impl Receipts {
    pub(super) fn new(channel:Arc<dyn DataChannel>)->Self {Self::Active(Box::new(Active {channel,phase:ChannelPhase::Connecting,ledger:Ledger::new(),window:Instant::now(),messages:0}))}
    pub(super) fn available(&self)->bool {matches!(self,Self::Active(active) if matches!(active.phase,ChannelPhase::Open))}
    pub(super) async fn poll(&self)->Option<DataChannelEvent> {
        match self {Self::Disabled=>std::future::pending().await,Self::Active(active)=>active.channel.poll().await}
    }
    pub(super) fn record(&mut self,stamp:VideoFrameStamp,pts:i64) {if let Self::Active(active)=self {active.ledger.record(stamp,pts,Instant::now());}}
    pub(super) fn retire(&mut self) {if let Self::Active(active)=self {active.ledger=Ledger::new();}}
    pub(super) fn event(&mut self,event:Option<DataChannelEvent>,generation:Option<u64>)->Result<Outcome,WebRtcError> {
        let Self::Active(active)=self else {return Ok(Outcome::None);};
        match event {
            Some(DataChannelEvent::OnOpen)=>{active.phase=ChannelPhase::Open;Ok(Outcome::None)},
            Some(DataChannelEvent::OnMessage(message))=>{
                if !matches!(active.phase,ChannelPhase::Open) {return Err(WebRtcError::InvalidFrame);}
                let now=Instant::now();
                if now.checked_duration_since(active.window).is_none_or(|age|age>=Duration::from_secs(1)) {active.window=now;active.messages=0;}
                active.messages=active.messages.checked_add(1).filter(|count|*count<=120).ok_or(WebRtcError::InvalidFrame)?;
                let stamp=decode(&message.data,message.is_string)?;
                Ok(generation.and_then(|generation|active.ledger.accept(stamp,generation,now)).map_or(Outcome::None,Outcome::Decoded))
            },
            None|Some(DataChannelEvent::OnClose|DataChannelEvent::OnClosing|DataChannelEvent::OnError)=>{*self=Self::Disabled;Ok(Outcome::Closed)},
            _=>Ok(Outcome::None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stamp(generation:u64,rtp_timestamp:u32)->VideoFrameStamp {VideoFrameStamp {generation,rtp_timestamp}}
    #[test]
    fn decoded_receipt_wire_is_fixed_binary_and_never_trusts_presentation_time() {
        let mut bytes=[0u8;12];bytes[..4].copy_from_slice(&MAGIC);bytes[4..8].copy_from_slice(&7u32.to_be_bytes());bytes[8..].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(decode(&bytes,false),Ok(stamp(7,u32::MAX)));
        assert!(decode(&bytes,true).is_err());assert!(decode(&bytes[..11],false).is_err());assert!(decode(&[0;13],false).is_err());bytes[0]=0;assert!(decode(&bytes,false).is_err());
        let options=options();assert!(!options.ordered);assert_eq!(options.max_retransmits,Some(0));assert_eq!(options.max_packet_life_time,None);
    }
    #[test]
    fn decoded_receipt_requires_recent_written_identity_and_forward_progress() {
        let now=Instant::now();let mut ledger=Ledger::new();ledger.record(stamp(3,10),100,now);ledger.record(stamp(3,11),200,now);
        assert!(ledger.accept(stamp(3,10),4,now).is_none());assert!(ledger.accept(stamp(3,12),3,now).is_none());
        assert_eq!(ledger.accept(stamp(3,11),3,now).map(|row|row.presentation_time_micros),Some(200));
        assert!(ledger.accept(stamp(3,11),3,now).is_none());assert!(ledger.accept(stamp(3,10),3,now).is_none());
    }
    #[test]
    fn decoded_receipt_expiry_clock_regression_and_rtp_collision_stay_unknown() {
        let now=Instant::now();let mut ledger=Ledger::new();ledger.record(stamp(0,0),0,now);
        assert!(ledger.accept(stamp(0,0),0,now+MAX_AGE+Duration::from_nanos(1)).is_none());
        assert!(ledger.accept(stamp(0,0),0,now-Duration::from_nanos(1)).is_none());
        ledger.record(stamp(0,0),1,now);assert!(ledger.accept(stamp(0,0),0,now).is_none());
        ledger.record(stamp(0,1),12,now);assert!(ledger.accept(stamp(0,1),0,now).is_some());
    }
    #[test]
    fn decoded_receipt_fixed_history_evicts_old_frames_and_handles_timestamp_wrap() {
        let now=Instant::now();let mut ledger=Ledger::new();
        for index in 0..=HISTORY {ledger.record(stamp(1,index as u32),index as i64,now);}
        assert!(ledger.accept(stamp(1,0),1,now).is_none());assert!(ledger.accept(stamp(1,64),1,now).is_some());
        let mut ledger=Ledger::new();ledger.record(stamp(1,u32::MAX),100,now);ledger.record(stamp(1,0),200,now);
        assert_eq!(ledger.accept(stamp(1,0),1,now).map(|row|row.presentation_time_micros),Some(200));
        ledger=Ledger::new();assert!(ledger.accept(stamp(1,0),1,now).is_none());
    }
    #[test]
    fn decoded_receipt_trickle_sdp_covers_bundled_application_without_touching_credentials() {
        let sdp="v=0\r\na=ice-options:renomination\r\nm=video 9 UDP/TLS/RTP/SAVPF 102\r\na=mid:0\r\na=ice-ufrag:secret\r\na=end-of-candidates\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\na=mid:1\r\n";
        let offer=super::super::trickle_offer(sdp);assert!(offer.starts_with("v=0\r\na=ice-options:renomination trickle\r\nm=video"));
        assert!(offer.contains("a=ice-ufrag:secret\r\n"));assert!(!offer.contains("a=end-of-candidates"));assert_eq!(super::super::trickle_offer(&offer),offer);
    }
}
