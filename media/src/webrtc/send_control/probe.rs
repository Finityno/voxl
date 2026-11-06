//! A requested quiet-resume probe, never periodic keepalive/video traffic.
//! Exact original departures bind receiver reports; no payloads are retained.
use super::*;
pub(in crate::webrtc) const PACKETS:usize=32;
pub(in crate::webrtc) const DEADLINE:Duration=Duration::from_millis(500);
#[derive(Clone,Copy)]
pub(super) struct ProbePacket {pub(super) index:usize}
#[derive(Clone,Copy)]
enum Slot {Unsent,Sent {ssrc:u32,sequence:u16,departure:Instant},Arrived {arrival:Duration,bytes:u32},Lost}
pub(super) struct Collection {slots:[Slot;PACKETS],deadline:Instant,ceiling:u32,ssrc:u32,sequence:u16}
#[derive(Debug,Clone,Copy)]
pub struct ResumeProbeObservation {pub packets:u32,pub arrived:u32,pub received_rtp_bytes:u32,pub arrival_span:Duration,pub received_bps:u32,pub target_kbps:u32}
pub(super) enum Probe {Idle,Collecting(Box<Collection>),Complete {observation:ResumeProbeObservation,at:Instant},Failed}
impl Probe {
    pub(super) fn begin(&mut self,now:Instant,ceiling:u32,ssrc:u32,sequence:u16)->bool {
        if !matches!(self,Self::Idle) {return false;}
        *self=Self::Collecting(Box::new(Collection {slots:[Slot::Unsent;PACKETS],deadline:now+DEADLINE,ceiling,ssrc,sequence}));true
    }
    pub(super) fn packet_index(&self,packet:&rtc::rtp::Packet)->Option<usize> {
        let Self::Collecting(collection)=self else {return None;};
        let index=usize::from(packet.header.sequence_number.wrapping_sub(collection.sequence));
        (packet.header.ssrc==collection.ssrc && index<PACKETS && packet.header.padding && !packet.header.marker && packet.payload.len()==251 && packet.payload.iter().all(|byte|*byte==0)).then_some(index)
    }
    pub(super) fn sent(&mut self,index:usize,packet:&rtc::rtp::Packet,now:Instant)->Result<(),()> {
        let Self::Collecting(collection)=self else {return Err(());};
        if now>collection.deadline || index>=PACKETS || !matches!(collection.slots[index],Slot::Unsent) || !packet.header.padding || packet.payload.iter().any(|byte|*byte!=0) {return Err(());}
        collection.slots[index]=Slot::Sent {ssrc:packet.header.ssrc,sequence:packet.header.sequence_number,departure:now};Ok(())
    }
    pub(super) fn feedback(&mut self,reports:&[PacketReport],now:Instant,min:u32)->Result<Option<ResumeProbeObservation>,()> {
        let Self::Collecting(collection)=self else {return Ok(None);};
        if now>collection.deadline {*self=Self::Failed;return Err(());}
        for report in reports {
            let Some(slot)=collection.slots.iter_mut().find(|slot|matches!(slot,Slot::Sent {ssrc,sequence,departure} if *ssrc==report.ssrc && *sequence==report.rtp_sequence_number && *departure==report.departure)) else {continue;};
            *slot=if report.arrived {match report.arrival {Some(arrival)=>Slot::Arrived {arrival,bytes:report.size.try_into().map_err(|_|())?},None=>{*self=Self::Failed;return Err(());}}} else {Slot::Lost};
        }
        if collection.slots.iter().any(|slot|matches!(slot,Slot::Unsent|Slot::Sent {..})) {return Ok(None);}
        // Require a full, loss-free bounded cluster. Lost/unknown reports are
        // not a measured zero or permission to reuse the old capacity.
        let mut first=None;let mut last=None;let mut bytes=0u32;let mut first_bytes=0u32;
        for slot in collection.slots {
            let Slot::Arrived {arrival,bytes:packet_bytes}=slot else {*self=Self::Failed;return Err(());};
            if first.is_none_or(|old|arrival<old) {first=Some(arrival);first_bytes=packet_bytes;}
            last=Some(last.map_or(arrival,|old:Duration|old.max(arrival)));bytes=bytes.checked_add(packet_bytes).ok_or(())?;
        }
        let span=last.zip(first).and_then(|(last,first)|last.checked_sub(first)).filter(|span|!span.is_zero()).ok_or(())?;
        let measured=u128::from(bytes-first_bytes)*8_000_000_000/span.as_nanos();
        let received_bps=u32::try_from(measured.min(u128::from(u32::MAX))).map_err(|_|())?;
        let bps=received_bps.clamp(min,collection.ceiling);
        let observation=ResumeProbeObservation {packets:PACKETS as u32,arrived:PACKETS as u32,received_rtp_bytes:bytes,arrival_span:span,received_bps,target_kbps:bps/1000};
        *self=Self::Complete {observation,at:now};Ok(Some(observation))
    }
    pub(super) fn observation(&self)->Result<Option<ResumeProbeObservation>,()> {match self {Self::Complete {observation,..}=>Ok(Some(*observation)),Self::Failed=>Err(()),_=>Ok(None)}}
    pub(super) fn cap(&self)->Option<u32> {match self {Self::Complete {observation,..}=>Some(observation.received_bps),_=>None}}
    pub(super) fn media_feedback(&mut self,reports:&[PacketReport],media:Option<(Instant,u32,u16)>) {
        if let Self::Complete {at,..}=self && let Some((departure,ssrc,sequence))=media && departure>*at
            && reports.iter().any(|report|report.arrived && report.departure==departure && report.ssrc==ssrc && report.rtp_sequence_number==sequence) {*self=Self::Idle;}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(sequence:u16)->rtc::rtp::Packet {rtc::rtp::Packet {header:rtc::rtp::Header {ssrc:55,sequence_number:sequence,padding:true,..Default::default()},payload:Bytes::new()}}
    fn report(now:Instant,sequence:u16)->PacketReport {PacketReport {ssrc:55,id:u64::from(sequence),rtp_sequence_number:sequence,is_twcc:true,twcc_sequence_number:sequence,size:24,arrived:true,departure:now,arrival:Some(Duration::from_micros(u64::from(sequence)*1000)),ecn:Default::default()}}
    #[test]
    fn resume_probe_matches_departure_epoch_and_consumes_reports_once() {
        let now=Instant::now();let mut probe=Probe::Idle;assert!(probe.begin(now,1_500_000,55,0));assert!(!probe.begin(now,1_500_000,55,0));
        for index in 0..PACKETS {probe.sent(index,&packet(index as u16),now).unwrap();}
        let mut wrong=report(now,0);wrong.departure=now-Duration::from_millis(1);assert!(probe.feedback(&[wrong],now,250_000).unwrap().is_none());
        let reports:Vec<_>=(0..PACKETS as u16).map(|sequence|report(now,sequence)).collect();
        assert!(probe.feedback(&reports[..16],now,250_000).unwrap().is_none());assert!(probe.feedback(&reports[..16],now,250_000).unwrap().is_none());
        let observed=probe.feedback(&reports[16..],now,250_000).unwrap().unwrap();assert_eq!((observed.packets,observed.arrived,observed.target_kbps),(32,32,250));assert_eq!(observed.arrival_span,Duration::from_millis(31));
        assert_eq!(probe.cap(),Some(192_000));assert!(probe.feedback(&reports,now,250_000).unwrap().is_none());probe.media_feedback(&reports,None);assert!(probe.cap().is_some());
        let mut new=report(now,0);new.departure=now+Duration::from_nanos(1);probe.media_feedback(&[new.clone()],Some((new.departure,55,1)));assert!(probe.cap().is_some());
        probe.media_feedback(std::slice::from_ref(&new),Some((new.departure,55,0)));assert!(probe.cap().is_none());
    }
    #[test]
    fn resume_probe_loss_missing_or_late_feedback_never_qualifies() {
        let now=Instant::now();
        for fault in 0..3 {
            let mut probe=Probe::Idle;assert!(probe.begin(now,1_500_000,55,0));for index in 0..PACKETS {probe.sent(index,&packet(index as u16),now).unwrap();}
            let mut reports:Vec<_>=(0..PACKETS as u16).map(|sequence|report(now,sequence)).collect();
            let at=if fault==2 {now+DEADLINE+Duration::from_nanos(1)} else {now};
            if fault==0 {reports[0].arrived=false;reports[0].arrival=None;}else if fault==1 {reports[0].arrival=None;}
            assert!(probe.feedback(&reports,at,250_000).is_err());assert!(probe.observation().is_err());assert!(probe.cap().is_none());
        }
    }
    #[test]
    fn resume_padding_wire_is_empty_at_receiver_and_ordinary_rtp_is_unchanged() {
        use rtc::shared::marshal::{Marshal,Unmarshal};
        let mut padding=packet(7);padding.header.version=2;padding.payload=Bytes::from(vec![0;251]);
        let ordinary=padding.marshal().unwrap();let mut wire=rtc::rtp_transceiver::RtpPaddingOnly::marshal(&padding).unwrap();assert_eq!(wire.len(),ordinary.len());
        assert_eq!(wire[wire.len()-1],252);let received=rtc::rtp::Packet::unmarshal(&mut wire).unwrap();assert!(received.payload.is_empty());assert!(received.header.padding && !received.header.marker);
        let mut original=ordinary;assert_eq!(rtc::rtp::Packet::unmarshal(&mut original).unwrap().payload.len(),251);
        for fault in 0..4 {
            let mut invalid=padding.clone();match fault {0=>invalid.header.padding=false,1=>invalid.header.marker=true,2=>invalid.payload=Bytes::from(vec![0;256]),_=>invalid.payload=Bytes::from(vec![1;251])}
            assert!(rtc::rtp_transceiver::RtpPaddingOnly::marshal(&invalid).is_err());
        }
    }
}
