//! Opt-in bounded metadata ledger. Pending feedback is not exact network
//! flight: lost, late, duplicated or untracked reports make that distinction
//! essential. No payloads, addresses or receiver absolute clocks are retained.
use super::*;
const RETAINED:usize=512;
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct FlowObservation {
    pub epoch:u64,pub released_packets:u64,pub released_bytes:u64,pub retransmissions:u64,
    pub feedback_batches:u64,pub feedback_reports:u64,pub arrived_reports:u64,pub lost_reports:u64,
    pub matched_reports:u64,pub unmatched_reports:u64,pub evicted_packets:u64,pub duplicate_releases:u64,
    pub pending_packets:u64,pub pending_bytes:u64,pub oldest_pending_micros:u64,
    pub feedback_age_micros:Option<u64>,pub feedback_newest_micros:u64,pub feedback_oldest_micros:u64,
    pub completed_frames:u64,pub incomplete_frames:u64,pub last_frame_bytes:u64,pub last_frame_pacing_micros:u64,pub peak_frame_bytes:u64,
    pub delay_groups:u64,pub delay_signals:u64,pub delay_resets:u64,
}
#[derive(Clone,Copy,PartialEq,Eq,Hash)]
struct Key {ssrc:u32,sequence:u16,departure:Instant}
enum Feedback {Unknown,At(Instant)}
enum Frame {Empty,Running {timestamp:u32,started:Instant,bytes:u64}}
pub(super) enum Monitor {Disabled,Tracking(Box<Tracker>)}
pub(super) struct Tracker {pending:HashMap<Key,usize>,order:VecDeque<Key>,feedback:Feedback,frame:Frame,counters:FlowObservation}
fn micros(value:Duration)->u64 {value.as_micros().try_into().unwrap_or(u64::MAX)}
impl Monitor {
    pub(super) fn new(enabled:bool)->Self {
        if enabled {Self::Tracking(Box::new(Tracker {pending:HashMap::with_capacity(RETAINED),order:VecDeque::with_capacity(RETAINED),feedback:Feedback::Unknown,frame:Frame::Empty,counters:FlowObservation::default()}))} else {Self::Disabled}
    }
    pub(super) fn released(&mut self,message:&TaggedPacket) {
        let Self::Tracking(tracker)=self else {return;};let Packet::Rtp(packet)=&message.message.packet else {return;};
        let key=Key {ssrc:packet.header.ssrc,sequence:packet.header.sequence_number,departure:message.now};let bytes=packet.marshal_size();
        let c=&mut tracker.counters;c.released_packets=c.released_packets.saturating_add(1);c.released_bytes=c.released_bytes.saturating_add(bytes as u64);
        if tracker.pending.contains_key(&key) {c.duplicate_releases=c.duplicate_releases.saturating_add(1);} else {
            if tracker.order.len()==RETAINED && let Some(old)=tracker.order.pop_front() && tracker.pending.remove(&old).is_some() {c.evicted_packets=c.evicted_packets.saturating_add(1);}
            tracker.pending.insert(key,bytes);tracker.order.push_back(key);
        }
        if message.message.has(&Attribute::Retransmission) {c.retransmissions=c.retransmissions.saturating_add(1);return;}
        if message.message.custom::<rtc::rtp_transceiver::RtpPaddingOnly>().is_some() {return;}
        if matches!(tracker.frame,Frame::Running {timestamp,..} if timestamp!=packet.header.timestamp) {c.incomplete_frames=c.incomplete_frames.saturating_add(1);tracker.frame=Frame::Empty;}
        if matches!(tracker.frame,Frame::Empty) {tracker.frame=Frame::Running {timestamp:packet.header.timestamp,started:message.now,bytes:0};}
        if let Frame::Running {started,bytes:total,..}=&mut tracker.frame {
            *total=total.saturating_add(bytes as u64);
            if packet.header.marker {c.completed_frames=c.completed_frames.saturating_add(1);c.last_frame_bytes = *total;c.peak_frame_bytes=c.peak_frame_bytes.max(*total);c.last_frame_pacing_micros=micros(message.now.saturating_duration_since(*started));tracker.frame=Frame::Empty;}
        }
    }
    pub(super) fn feedback(&mut self,now:Instant,reports:&[PacketReport],delay:(u64,u64,u64)) {
        let Self::Tracking(tracker)=self else {return;};if reports.is_empty() {return;}
        tracker.feedback=Feedback::At(now);let c=&mut tracker.counters;c.feedback_batches=c.feedback_batches.saturating_add(1);c.feedback_newest_micros=u64::MAX;c.feedback_oldest_micros=0;
        (c.delay_groups,c.delay_signals,c.delay_resets)=delay;
        for report in reports {
            c.feedback_reports=c.feedback_reports.saturating_add(1);
            if report.arrived {c.arrived_reports=c.arrived_reports.saturating_add(1);} else {c.lost_reports=c.lost_reports.saturating_add(1);}
            let age=micros(now.saturating_duration_since(report.departure));c.feedback_newest_micros=c.feedback_newest_micros.min(age);c.feedback_oldest_micros=c.feedback_oldest_micros.max(age);
            let key=Key {ssrc:report.ssrc,sequence:report.rtp_sequence_number,departure:report.departure};
            if tracker.pending.remove(&key).is_some() {c.matched_reports=c.matched_reports.saturating_add(1);} else {c.unmatched_reports=c.unmatched_reports.saturating_add(1);}
        }
    }
    pub(super) fn snapshot(&self,now:Instant,epoch:u64)->Option<FlowObservation> {
        let Self::Tracking(tracker)=self else {return None;};let mut value=tracker.counters;value.epoch=epoch;
        value.pending_packets=tracker.pending.len() as u64;value.pending_bytes=tracker.pending.values().map(|bytes|*bytes as u64).sum();
        value.oldest_pending_micros=tracker.pending.keys().map(|key|micros(now.saturating_duration_since(key.departure))).max().unwrap_or(0);
        value.feedback_age_micros=match tracker.feedback {Feedback::Unknown=>None,Feedback::At(at)=>Some(micros(now.saturating_duration_since(at)))};Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(now:Instant,sequence:u16,marker:bool,rtx:bool)->TaggedPacket {
        let mut message=rtc::interceptor::AttributedPacket::new(Packet::Rtp(rtc::rtp::Packet {header:rtc::rtp::header::Header {version:2,ssrc:55,sequence_number:sequence,timestamp:900,marker,..Default::default()},payload:vec![1;1000].into()}));
        if rtx {message.add(Attribute::Retransmission);}TaggedPacket {now,transport:Default::default(),message}
    }
    fn report(now:Instant,sequence:u16,arrived:bool)->PacketReport {PacketReport {ssrc:55,id:u64::from(sequence),rtp_sequence_number:sequence,is_twcc:true,twcc_sequence_number:sequence,size:1020,arrived,departure:now,arrival:arrived.then_some(Duration::from_millis(1)),ecn:Default::default()}}
    #[test]
    fn flow_observation_matches_rtx_by_departure_without_double_counting_feedback() {
        let now=Instant::now();let mut monitor=Monitor::new(true);monitor.released(&packet(now,1,false,false));monitor.released(&packet(now+Duration::from_millis(1),1,true,true));
        let before=monitor.snapshot(now+Duration::from_millis(2),7).unwrap();assert_eq!((before.pending_packets,before.retransmissions),(2,1));assert_eq!(before.completed_frames,0);
        monitor.feedback(now+Duration::from_millis(100),&[report(now,1,false)],(1,0,1));let middle=monitor.snapshot(now+Duration::from_millis(105),7).unwrap();assert_eq!((middle.pending_packets,middle.lost_reports,middle.matched_reports),(1,1,1));assert_eq!(middle.feedback_age_micros,Some(5000));assert_eq!(middle.feedback_oldest_micros,100000);
        monitor.feedback(now+Duration::from_millis(120),&[report(now,1,true),report(now+Duration::from_millis(1),1,true)],(3,1,1));let after=monitor.snapshot(now+Duration::from_millis(125),7).unwrap();assert_eq!((after.pending_packets,after.unmatched_reports,after.matched_reports),(0,1,2));assert_eq!(after.pending_bytes,0);assert_eq!(after.delay_signals,1);
    }
    #[test]
    fn flow_observation_retention_is_bounded_and_eviction_is_visible() {
        let now=Instant::now();let mut monitor=Monitor::new(true);
        for sequence in 0..600 {monitor.released(&packet(now,sequence,false,false));}
        let value=monitor.snapshot(now+Duration::from_secs(1),0).unwrap();assert_eq!((value.pending_packets,value.evicted_packets),(512,88));assert_eq!(value.oldest_pending_micros,1_000_000);assert!(value.feedback_age_micros.is_none());
        monitor.feedback(now+Duration::from_secs(1),&[report(now,0,true)],(0,0,0));assert_eq!(monitor.snapshot(now,0).unwrap().unmatched_reports,1);
    }
    #[test]
    fn flow_observation_marker_closes_only_original_frame_and_disabled_is_inert() {
        let now=Instant::now();let mut monitor=Monitor::new(true);monitor.released(&packet(now,1,false,false));monitor.released(&packet(now+Duration::from_millis(10),2,true,false));
        let value=monitor.snapshot(now,0).unwrap();assert_eq!(value.completed_frames,1);assert_eq!(value.last_frame_bytes,2024);assert_eq!(value.last_frame_pacing_micros,10000);
        let mut disabled=Monitor::new(false);disabled.released(&packet(now,1,true,false));disabled.feedback(now,&[report(now,1,true)],(1,1,1));assert!(disabled.snapshot(now,0).is_none());
    }
}
