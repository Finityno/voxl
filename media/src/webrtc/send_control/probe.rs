//! A requested quiet-resume probe, never periodic keepalive/video traffic.
//! Exact original departures bind receiver reports; no payloads are retained.
use super::*;
pub(in crate::webrtc) const PACKETS:usize=32;
pub(in crate::webrtc) const DEADLINE:Duration=Duration::from_millis(500);
const FRESHNESS:Duration=Duration::from_secs(2);
#[derive(Clone,Copy)]
pub(super) struct ProbePacket {pub(super) index:usize}
#[derive(Debug,Clone,Copy)]
pub(super) enum Policy {Disabled,Single,RetryOnce}
impl Policy {pub(super) fn enabled(self)->bool {!matches!(self,Self::Disabled)}}
#[derive(Clone,Copy)]
enum Attempt {Single,First,Retry}
pub(super) enum Feedback {Pending,Retry,Complete(ResumeProbeObservation)}
pub(in crate::webrtc) enum Outcome {Retry,Complete(ResumeProbeObservation)}
#[derive(Clone,Copy)]
enum Slot {Unsent,Sent {ssrc:u32,sequence:u16,departure:Instant},Arrived {arrival:Duration,bytes:u32},Lost}
pub(super) struct Collection {slots:[Slot;PACKETS],ticket:ResumeProbeTicket,ceiling:u32,ssrc:u32,sequence:u16,attempt:Attempt,transmitted_rtp_bytes:u32}
#[derive(Debug,Clone,Copy)]
pub struct ResumeProbeObservation {pub packets:u32,pub arrived:u32,pub received_rtp_bytes:u32,pub arrival_span:Duration,pub received_bps:u32,pub target_kbps:u32,pub attempts:u32,pub transmitted_rtp_bytes:u32}
/// Opaque current-epoch request; neither view nor input authority is granted.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ResumeProbeTicket {pub(super) epoch:u64,serial:u64,deadline:Instant}
impl ResumeProbeTicket {
    pub(super) fn new(epoch:u64,serial:u64,now:Instant)->Self {Self {epoch,serial,deadline:now+DEADLINE}}
    pub(super) fn remaining(self,now:Instant)->Option<Duration> {self.deadline.checked_duration_since(now).filter(|wait|!wait.is_zero())}
    pub(in crate::webrtc) fn deadline(self)->Instant {self.deadline}
}
/// Retired requests cannot publish the output held for their previous epoch.
#[derive(Debug,Clone,Copy)]
pub enum ResumeProbeStatus {Pending,Ready(ResumeProbeObservation),Retired}
#[derive(Clone,Copy)]
pub(super) enum Dispatch {Waiting,Claimed}
pub(super) enum Probe {Idle,Quiet,Requested {ticket:ResumeProbeTicket,dispatch:Dispatch},Collecting(Box<Collection>),Retry(Box<Collection>),Complete {ticket:ResumeProbeTicket,observation:ResumeProbeObservation,at:Instant},Failed}
impl Probe {
    pub(super) fn quiet(&mut self,now:Instant) {
        if matches!(self,Self::Idle) || matches!(self,Self::Complete {at,..} if now.saturating_duration_since(*at)>FRESHNESS) {*self=Self::Quiet;}
    }
    pub(super) fn unprobed(&self)->bool {matches!(self,Self::Quiet)}
    pub(super) fn request(&mut self,ticket:ResumeProbeTicket,now:Instant)->bool {
        if !matches!(self,Self::Quiet|Self::Idle) && !matches!(self,Self::Complete {at,..} if now.saturating_duration_since(*at)>FRESHNESS) {return false;}
        *self=Self::Requested {ticket,dispatch:Dispatch::Waiting};true
    }
    pub(super) fn pending(&self)->Option<ResumeProbeTicket> {match self {Self::Requested {ticket,..}=>Some(*ticket),Self::Collecting(collection)|Self::Retry(collection)=>Some(collection.ticket),_=>None}}
    pub(super) fn take_request(&mut self)->Option<ResumeProbeTicket> {
        match self {Self::Requested {ticket,dispatch:Dispatch::Waiting}=>{let ticket=*ticket;*self=Self::Requested {ticket,dispatch:Dispatch::Claimed};Some(ticket)},_=>None}
    }
    pub(super) fn begin(&mut self,ticket:ResumeProbeTicket,now:Instant,ceiling:u32,ssrc:u32,sequence:u16,policy:Policy)->bool {
        if !matches!(self,Self::Requested {ticket:current,..} if *current==ticket) || ticket.remaining(now).is_none() {return false;}
        *self=Self::Collecting(Box::new(Collection {slots:[Slot::Unsent;PACKETS],ticket,ceiling,ssrc,sequence,attempt:if matches!(policy,Policy::RetryOnce) {Attempt::First} else {Attempt::Single},transmitted_rtp_bytes:0}));true
    }
    pub(super) fn retry_pending(&self,ticket:ResumeProbeTicket)->bool {matches!(self,Self::Retry(collection) if collection.ticket==ticket)}
    pub(super) fn begin_retry(&mut self,ticket:ResumeProbeTicket,now:Instant,ceiling:u32,ssrc:u32,sequence:u16)->bool {
        let Self::Retry(collection)=self else {return false;};
        if collection.ticket!=ticket || ticket.remaining(now).is_none() || ssrc!=collection.ssrc || sequence!=collection.sequence.wrapping_add(PACKETS as u16) {return false;}
        let Self::Retry(mut collection)=std::mem::replace(self,Self::Failed) else {return false;};
        collection.slots=[Slot::Unsent;PACKETS];collection.ceiling=collection.ceiling.min(ceiling);collection.sequence=sequence;collection.attempt=Attempt::Retry;
        *self=Self::Collecting(collection);true
    }
    pub(super) fn packet_index(&self,packet:&rtc::rtp::Packet)->Option<usize> {
        let Self::Collecting(collection)=self else {return None;};
        let index=usize::from(packet.header.sequence_number.wrapping_sub(collection.sequence));
        (packet.header.ssrc==collection.ssrc && index<PACKETS && packet.header.padding && !packet.header.marker && packet.payload.len()==251 && packet.payload.iter().all(|byte|*byte==0)).then_some(index)
    }
    pub(super) fn sent(&mut self,index:usize,packet:&rtc::rtp::Packet,now:Instant)->Result<(),()> {
        let Self::Collecting(collection)=self else {return Err(());};
        if collection.ticket.remaining(now).is_none() || index>=PACKETS || !matches!(collection.slots[index],Slot::Unsent) || !packet.header.padding || packet.payload.iter().any(|byte|*byte!=0) {return Err(());}
        collection.transmitted_rtp_bytes=collection.transmitted_rtp_bytes.checked_add(packet.marshal_size().try_into().map_err(|_|())?).ok_or(())?;
        collection.slots[index]=Slot::Sent {ssrc:packet.header.ssrc,sequence:packet.header.sequence_number,departure:now};Ok(())
    }
    pub(super) fn feedback(&mut self,reports:&[PacketReport],now:Instant,min:u32)->Result<Feedback,()> {
        let Self::Collecting(collection)=self else {return Ok(Feedback::Pending);};
        if collection.ticket.remaining(now).is_none() {*self=Self::Failed;return Err(());}
        for report in reports {
            let Some(slot)=collection.slots.iter_mut().find(|slot|matches!(slot,Slot::Sent {ssrc,sequence,departure} if *ssrc==report.ssrc && *sequence==report.rtp_sequence_number && *departure==report.departure)) else {continue;};
            *slot=if report.arrived {match report.arrival {Some(arrival)=>Slot::Arrived {arrival,bytes:report.size.try_into().map_err(|_|())?},None=>{*self=Self::Failed;return Err(());}}} else {Slot::Lost};
        }
        if collection.slots.iter().any(|slot|matches!(slot,Slot::Unsent|Slot::Sent {..})) {return Ok(Feedback::Pending);}
        if collection.slots.iter().any(|slot|matches!(slot,Slot::Lost)) {
            let retry=matches!(collection.attempt,Attempt::First) && collection.ticket.remaining(now).is_some();
            let Self::Collecting(collection)=std::mem::replace(self,Self::Failed) else {return Err(());};
            if retry {*self=Self::Retry(collection);return Ok(Feedback::Retry);}return Err(());
        }
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
        let observation=ResumeProbeObservation {packets:PACKETS as u32,arrived:PACKETS as u32,received_rtp_bytes:bytes,arrival_span:span,received_bps,target_kbps:bps/1000,attempts:if matches!(collection.attempt,Attempt::Retry) {2} else {1},transmitted_rtp_bytes:collection.transmitted_rtp_bytes};
        *self=Self::Complete {ticket:collection.ticket,observation,at:now};Ok(Feedback::Complete(observation))
    }
    #[cfg(test)]
    pub(super) fn observation(&self)->Result<Option<ResumeProbeObservation>,()> {match self {Self::Complete {observation,..}=>Ok(Some(*observation)),Self::Failed=>Err(()),_=>Ok(None)}}
    pub(super) fn poll(&self,ticket:ResumeProbeTicket,now:Instant)->Result<ResumeProbeStatus,WebRtcError> {
        match self {
            Self::Complete {ticket:current,observation,..} if *current==ticket=>if ticket.remaining(now).is_some() {Ok(ResumeProbeStatus::Ready(*observation))} else {Err(WebRtcError::Deadline)},
            Self::Requested {ticket:current,..} if *current==ticket=>if ticket.remaining(now).is_some() {Ok(ResumeProbeStatus::Pending)} else {Err(WebRtcError::Deadline)},
            Self::Collecting(collection)|Self::Retry(collection) if collection.ticket==ticket=>if ticket.remaining(now).is_some() {Ok(ResumeProbeStatus::Pending)} else {Err(WebRtcError::Deadline)},
            Self::Failed=>Err(WebRtcError::Transport),_=>Ok(ResumeProbeStatus::Retired),
        }
    }
    pub(super) fn cap(&self,now:Instant,minimum:u32)->Option<u32> {
        match self {Self::Complete {observation,at,..}=>Some(if now.saturating_duration_since(*at)<=FRESHNESS {observation.received_bps} else {observation.received_bps.min(minimum)}),_=>None}
    }
    pub(super) fn media_feedback(&mut self,reports:&[PacketReport],media:Option<(Instant,u32,u16)>) {
        if let Self::Complete {at,..}=self && let Some((departure,ssrc,sequence))=media && departure>*at
            && reports.iter().any(|report|report.arrived && report.departure==departure && report.ssrc==ssrc && report.rtp_sequence_number==sequence) {*self=Self::Idle;}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(sequence:u16)->rtc::rtp::Packet {rtc::rtp::Packet {header:rtc::rtp::Header {ssrc:55,sequence_number:sequence,padding:true,..Default::default()},payload:Bytes::new()}}
    fn start(probe:&mut Probe,now:Instant,ceiling:u32,ssrc:u32,sequence:u16)->bool {
        let ticket=ResumeProbeTicket::new(0,1,now);probe.request(ticket,now) && probe.begin(ticket,now,ceiling,ssrc,sequence,Policy::Single)
    }
    fn report(now:Instant,sequence:u16)->PacketReport {PacketReport {ssrc:55,id:u64::from(sequence),rtp_sequence_number:sequence,is_twcc:true,twcc_sequence_number:sequence,size:24,arrived:true,departure:now,arrival:Some(Duration::from_micros(u64::from(sequence)*1000)),ecn:Default::default()}}
    fn lossy_first(now:Instant)->(Probe,ResumeProbeTicket) {
        let mut probe=Probe::Idle;let ticket=ResumeProbeTicket::new(0,1,now);assert!(probe.request(ticket,now));assert!(probe.begin(ticket,now,1_500_000,55,65_520,Policy::RetryOnce));
        let mut reports=Vec::new();for index in 0..PACKETS as u16 {let sequence=65_520u16.wrapping_add(index);probe.sent(usize::from(index),&packet(sequence),now).unwrap();reports.push(report(now,sequence));}
        reports[4].arrived=false;reports[4].arrival=None;assert!(matches!(probe.feedback(&reports,now+Duration::from_millis(100),250_000).unwrap(),Feedback::Retry));(probe,ticket)
    }
    #[test]
    fn resume_probe_retry_uses_fresh_slots_original_ticket_and_tightened_ceiling() {
        let now=Instant::now();let (mut probe,ticket)=lossy_first(now);assert!(probe.retry_pending(ticket));assert_eq!(probe.pending(),Some(ticket));assert!(probe.cap(now,250_000).is_none());
        assert!(!probe.begin_retry(ResumeProbeTicket::new(0,2,now),now,1_500_000,55,16));assert!(!probe.begin_retry(ticket,now,1_500_000,55,15));
        let later=now+Duration::from_millis(200);assert!(probe.begin_retry(ticket,later,300_000,55,16));
        let mut reports=Vec::new();for index in 0..PACKETS as u16 {let sequence=16+index;probe.sent(usize::from(index),&packet(sequence),later).unwrap();let mut value=report(later,sequence);value.arrival=Some(Duration::from_micros(u64::from(index)*100));reports.push(value);}
        let old:Vec<_>=(0..PACKETS as u16).map(|index|report(now,65_520u16.wrapping_add(index))).collect();assert!(matches!(probe.feedback(&old,later,250_000).unwrap(),Feedback::Pending));
        let Feedback::Complete(observed)=probe.feedback(&reports,later,250_000).unwrap() else {panic!("fresh complete retry");};assert_eq!((observed.attempts,observed.packets,observed.arrived,observed.target_kbps),(2,32,32,300));assert_eq!(observed.transmitted_rtp_bytes,64*packet(0).marshal_size() as u32);
        assert!(matches!(probe.poll(ticket,later).unwrap(),ResumeProbeStatus::Ready(_)));assert!(matches!(probe.poll(ticket,ticket.deadline()),Err(WebRtcError::Deadline)));
    }
    #[test]
    fn resume_probe_retry_never_extends_deadline_or_allows_a_third_attempt() {
        let now=Instant::now();
        for late in [false,true] {
            let (mut probe,ticket)=lossy_first(now);let later=now+Duration::from_millis(400);assert!(probe.begin_retry(ticket,later,1_500_000,55,16));assert_eq!(ticket.remaining(later),Some(Duration::from_millis(100)));
            let mut reports=Vec::new();for index in 0..PACKETS as u16 {probe.sent(usize::from(index),&packet(16+index),later).unwrap();reports.push(report(later,16+index));}
            if !late {reports[4].arrived=false;reports[4].arrival=None;}
            let at=if late {ticket.deadline()} else {later};assert!(probe.feedback(&reports,at,250_000).is_err());assert!(probe.observation().is_err());assert!(!probe.retry_pending(ticket));assert!(!probe.begin_retry(ticket,at,1_500_000,55,48));assert!(probe.cap(at,250_000).is_none());
        }
    }
    #[test]
    fn resume_probe_retry_does_not_accept_missing_or_malformed_first_feedback() {
        let now=Instant::now();
        for malformed in [false,true] {
            let mut probe=Probe::Idle;let ticket=ResumeProbeTicket::new(0,1,now);assert!(probe.request(ticket,now));assert!(probe.begin(ticket,now,1_500_000,55,0,Policy::RetryOnce));
            for index in 0..PACKETS {probe.sent(index,&packet(index as u16),now).unwrap();}
            let mut reports:Vec<_>=(0..PACKETS as u16).map(|index|report(now,index)).collect();
            if malformed {reports[4].arrival=None;assert!(probe.feedback(&reports,now,250_000).is_err());} else {assert!(matches!(probe.feedback(&reports[..31],now,250_000).unwrap(),Feedback::Pending));assert!(matches!(probe.poll(ticket,ticket.deadline()),Err(WebRtcError::Deadline)));}
            assert!(!probe.retry_pending(ticket));assert!(!probe.begin_retry(ticket,now,1_500_000,55,32));assert!(probe.cap(now,250_000).is_none());
        }
    }
    #[test]
    fn resume_probe_matches_departure_epoch_and_consumes_reports_once() {
        let now=Instant::now();let mut probe=Probe::Idle;assert!(start(&mut probe,now,1_500_000,55,0));assert!(!start(&mut probe,now,1_500_000,55,0));
        for index in 0..PACKETS {probe.sent(index,&packet(index as u16),now).unwrap();}
        let mut wrong=report(now,0);wrong.departure=now-Duration::from_millis(1);assert!(matches!(probe.feedback(&[wrong],now,250_000).unwrap(),Feedback::Pending));
        let reports:Vec<_>=(0..PACKETS as u16).map(|sequence|report(now,sequence)).collect();
        assert!(matches!(probe.feedback(&reports[..16],now,250_000).unwrap(),Feedback::Pending));assert!(matches!(probe.feedback(&reports[..16],now,250_000).unwrap(),Feedback::Pending));
        let Feedback::Complete(observed)=probe.feedback(&reports[16..],now,250_000).unwrap() else {panic!("complete exact probe");};assert_eq!((observed.packets,observed.arrived,observed.target_kbps),(32,32,250));assert_eq!(observed.arrival_span,Duration::from_millis(31));
        assert_eq!(probe.cap(now,250_000),Some(192_000));assert!(matches!(probe.feedback(&reports,now,250_000).unwrap(),Feedback::Pending));probe.media_feedback(&reports,None);assert!(probe.cap(now,250_000).is_some());
        let mut new=report(now,0);new.departure=now+Duration::from_nanos(1);probe.media_feedback(&[new.clone()],Some((new.departure,55,1)));assert!(probe.cap(now,250_000).is_some());
        probe.media_feedback(std::slice::from_ref(&new),Some((new.departure,55,0)));assert!(probe.cap(now,250_000).is_none());
    }
    #[test]
    fn resume_probe_loss_missing_or_late_feedback_never_qualifies() {
        let now=Instant::now();
        for fault in 0..3 {
            let mut probe=Probe::Idle;assert!(start(&mut probe,now,1_500_000,55,0));for index in 0..PACKETS {probe.sent(index,&packet(index as u16),now).unwrap();}
            let mut reports:Vec<_>=(0..PACKETS as u16).map(|sequence|report(now,sequence)).collect();
            let at=if fault==2 {now+DEADLINE+Duration::from_nanos(1)} else {now};
            if fault==0 {reports[0].arrived=false;reports[0].arrival=None;}else if fault==1 {reports[0].arrival=None;}
            assert!(probe.feedback(&reports,at,250_000).is_err());assert!(probe.observation().is_err());assert!(probe.cap(at,250_000).is_none());
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
    #[test]
    fn resume_probe_cap_expires_conservatively_and_a_later_probe_rejects_old_reports() {
        let now=Instant::now();let mut probe=Probe::Idle;assert!(start(&mut probe,now,1_500_000,55,0));
        for index in 0..PACKETS {probe.sent(index,&packet(index as u16),now).unwrap();}
        let reports:Vec<_>=(0..PACKETS as u16).map(|sequence|{let mut r=report(now,sequence);r.arrival=Some(Duration::from_micros(u64::from(sequence)*50));r}).collect();
        let Feedback::Complete(value)=probe.feedback(&reports,now,250_000).unwrap() else {panic!("complete exact probe");};assert_eq!(value.target_kbps,1500);assert!(probe.cap(now,250_000).unwrap()>1_500_000);
        let later=now+FRESHNESS+Duration::from_nanos(1);assert_eq!(probe.cap(later,250_000),Some(250_000));
        assert!(start(&mut probe,later,1_500_000,55,0));for index in 0..PACKETS {probe.sent(index,&packet(index as u16),later).unwrap();}
        assert!(matches!(probe.feedback(&reports,later,250_000).unwrap(),Feedback::Pending));assert!(probe.observation().unwrap().is_none());
    }
}
