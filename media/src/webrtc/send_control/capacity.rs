//! Bounded normal-RTP burst evidence. Sparse acknowledgements cannot refresh
//! the departure time of an older bulk transfer or count padding/RTX twice.
use super::*;
const SLOTS:usize=32;
const MIN_PACKETS:u32=8;
const MIN_BYTES:u32=8704;
#[derive(Clone,Copy)]
struct Identity {ssrc:u32,sequence:u16,timestamp:u32,departure:Instant}
#[derive(Clone,Copy)]
enum Slot {Empty,Pending(Identity),Arrived {identity:Identity,arrival:Duration,bytes:u32},Rejected}
#[derive(Clone,Copy)]
enum Evidence {Unknown,Measured {at:Instant,bps:u32}}
impl Evidence {
    fn newer(self,other:Self)->Self {match (self,other) {(Self::Measured {at:left,bps:a},Self::Measured {at:right,bps:b}) if left==right=>Self::Measured {at:left,bps:a.min(b)},(Self::Measured {at:left,..},Self::Measured {at:right,..}) if left>right=>self,(_,Self::Unknown)=>self,_=>other}}
    fn cap(self,now:Instant)->Option<u32> {match self {Self::Measured {at,bps} if now.checked_duration_since(at).is_some_and(|age|age<=probe::FRESHNESS)=>Some(bps),_=>None}}
}
pub(super) struct Window {slots:[Slot;SLOTS],next:usize,evidence:Evidence}
pub(super) enum Capacity {Disabled,Tracking(Box<Window>)}
impl Capacity {
    pub(super) fn new(enabled:bool)->Self {if enabled {Self::Tracking(Box::new(Window {slots:[Slot::Empty;SLOTS],next:0,evidence:Evidence::Unknown}))} else {Self::Disabled}}
    pub(super) fn sent(&mut self,packet:&rtc::rtp::Packet,now:Instant) {
        let Self::Tracking(window)=self else {return;};
        window.slots[window.next]=Slot::Pending(Identity {ssrc:packet.header.ssrc,sequence:packet.header.sequence_number,timestamp:packet.header.timestamp,departure:now});window.next=(window.next+1)%SLOTS;
    }
    pub(super) fn feedback(&mut self,reports:&[PacketReport],now:Instant) {
        let Self::Tracking(window)=self else {return;};
        for report in reports {
            let Some(slot)=window.slots.iter_mut().find(|slot|matches!(slot,Slot::Pending(id) if id.ssrc==report.ssrc && id.sequence==report.rtp_sequence_number && id.departure==report.departure)) else {continue;};
            let Slot::Pending(identity)=*slot else {continue;};
            *slot=match (report.arrived,report.arrival,u32::try_from(report.size)) {(true,Some(arrival),Ok(bytes))=>Slot::Arrived {identity,arrival,bytes},_=>Slot::Rejected};
        }
        let mut run=Run::Empty;
        for offset in 0..SLOTS {
            run=run.push(window.slots[(window.next+offset)%SLOTS],now);
            if let Some((at,bps))=run.measure() {window.evidence=window.evidence.newer(Evidence::Measured {at,bps});}
        }
    }
    pub(super) fn probed(&mut self,now:Instant,bps:u32) {if let Self::Tracking(window)=self {window.evidence=window.evidence.newer(Evidence::Measured {at:now,bps});}}
    pub(super) fn cap(&self,now:Instant)->Option<u32> {match self {Self::Disabled=>None,Self::Tracking(window)=>window.evidence.cap(now)}}
}
enum Run {Empty,Receiving {last:Identity,first:Duration,last_arrival:Duration,first_bytes:u32,bytes:u32,packets:u32}}
impl Run {
    fn push(self,slot:Slot,now:Instant)->Self {
        let Slot::Arrived {identity,arrival,bytes}=slot else {return Self::Empty;};
        if !now.checked_duration_since(identity.departure).is_some_and(|age|age<=probe::FRESHNESS) {return Self::Empty;}
        if let Self::Receiving {last,first,last_arrival,first_bytes,bytes:total,packets}=self
            && identity.ssrc==last.ssrc && identity.timestamp==last.timestamp && identity.sequence==last.sequence.wrapping_add(1) && identity.departure>=last.departure
            && let Some(total)=total.checked_add(bytes) {
            return Self::Receiving {last:identity,first:first.min(arrival),last_arrival:last_arrival.max(arrival),first_bytes:if arrival<first {bytes} else {first_bytes},bytes:total,packets:packets+1};
        }
        Self::Receiving {last:identity,first:arrival,last_arrival:arrival,first_bytes:bytes,bytes,packets:1}
    }
    fn measure(&self)->Option<(Instant,u32)> {
        let Self::Receiving {last,first,last_arrival,first_bytes,bytes,packets}=*self else {return None;};
        if packets<MIN_PACKETS || bytes<MIN_BYTES {return None;}
        let span=last_arrival.checked_sub(first).filter(|span|!span.is_zero())?;
        let bps=u128::from(bytes.checked_sub(first_bytes)?)*8_000_000_000/span.as_nanos();let bps=u32::try_from(bps.min(u128::from(u32::MAX))).ok()?;
        (bps>0).then_some((last.departure,bps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(index:u16,timestamp:u32)->rtc::rtp::Packet {rtc::rtp::Packet {header:rtc::rtp::Header {ssrc:55,sequence_number:index,timestamp,..Default::default()},payload:Bytes::from_static(&[1;1080])}}
    fn report(at:Instant,index:u16,arrival:u64)->PacketReport {PacketReport {ssrc:55,id:u64::from(index),rtp_sequence_number:index,is_twcc:true,twcc_sequence_number:index,size:1120,arrived:true,departure:at,arrival:Some(Duration::from_micros(arrival)),ecn:Default::default()}}
    #[test]
    fn normal_capacity_binds_contiguous_burst_departures_and_sparse_ack_cannot_refresh_it() {
        let now=Instant::now();let mut capacity=Capacity::new(true);let mut reports=Vec::new();
        for index in 0..12u16 {let at=now+Duration::from_millis(u64::from(index));capacity.sent(&packet(index,90000),at);reports.push(report(at,index,u64::from(index)*3000));}
        let end=now+Duration::from_millis(12);capacity.feedback(&reports,end);assert_eq!(capacity.cap(end),Some(2_986_666));
        let late=now+Duration::from_millis(2012);capacity.sent(&packet(12,180000),late);capacity.feedback(&[report(late,12,2_012_000)],late);assert!(capacity.cap(late).is_none());
        capacity.feedback(&reports,late);assert!(capacity.cap(late).is_none());
    }
    #[test]
    fn normal_capacity_never_combines_tiny_different_frames_into_bulk_evidence() {
        let now=Instant::now();let mut capacity=Capacity::new(true);let mut reports=Vec::new();
        for index in 0..32u16 {let at=now+Duration::from_millis(u64::from(index)*10);capacity.sent(&packet(index,u32::from(index)*3000),at);reports.push(report(at,index,u64::from(index)*10_000));}
        capacity.feedback(&reports,now+Duration::from_millis(400));assert!(capacity.cap(now+Duration::from_millis(400)).is_none());
    }
    #[test]
    fn normal_capacity_rejects_loss_missing_mismatched_or_zero_span_reports() {
        let now=Instant::now();
        for fault in 0..5 {
            let mut capacity=Capacity::new(true);let mut reports=Vec::new();
            for index in 0..12u16 {let at=now+Duration::from_millis(u64::from(index));capacity.sent(&packet(index,90000),at);reports.push(report(at,index,u64::from(index)*3000));}
            match fault {0=>reports[5].arrived=false,1=>reports[5].arrival=None,2=>reports[5].departure=now,3=>reports[5].ssrc=99,_=>{for value in &mut reports {value.arrival=Some(Duration::ZERO);}}}
            capacity.feedback(&reports,now+Duration::from_millis(12));assert!(capacity.cap(now+Duration::from_millis(12)).is_none());
        }
    }
    #[test]
    fn normal_capacity_wraps_sequence_but_not_epoch_or_departure_identity() {
        let now=Instant::now();let mut capacity=Capacity::new(true);let mut reports=Vec::new();
        for index in 0..12u16 {let sequence=65532u16.wrapping_add(index);let at=now+Duration::from_millis(u64::from(index));capacity.sent(&packet(sequence,90000),at);reports.push(report(at,sequence,u64::from(index)*3000));}
        capacity.feedback(&reports,now+Duration::from_millis(12));assert!(capacity.cap(now+Duration::from_millis(12)).is_some());
        capacity=Capacity::new(true);for index in 0..12u16 {capacity.sent(&packet(65532u16.wrapping_add(index),90000),now+Duration::from_secs(1)+Duration::from_millis(u64::from(index)));}
        capacity.feedback(&reports,now+Duration::from_millis(1012));assert!(capacity.cap(now+Duration::from_millis(1012)).is_none());
    }
    #[test]
    fn normal_capacity_coalesced_departures_can_tighten_but_never_refresh_or_raise_credit() {
        let now=Instant::now();let mut capacity=Capacity::new(true);let mut reports=Vec::new();
        for index in 0..12u16 {capacity.sent(&packet(index,90000),now);reports.push(report(now,index,if index<8 {u64::from(index)*1000} else {u64::from(index)*4000}));}
        capacity.feedback(&reports[..8],now+Duration::from_millis(10));assert_eq!(capacity.cap(now+Duration::from_millis(10)),Some(8_960_000));
        capacity.feedback(&reports[8..],now+Duration::from_millis(50));let full=2_240_000;assert_eq!(capacity.cap(now+Duration::from_millis(50)),Some(full));
        capacity.feedback(&reports[..8],now+Duration::from_secs(1));assert_eq!(capacity.cap(now+Duration::from_secs(1)),Some(full));assert!(capacity.cap(now+probe::FRESHNESS+Duration::from_nanos(1)).is_none());
    }
    #[test]
    fn normal_capacity_disabled_allocates_no_window_and_probe_evidence_expires() {
        let now=Instant::now();let mut disabled=Capacity::new(false);disabled.sent(&packet(0,0),now);disabled.probed(now,3_000_000);assert!(matches!(disabled,Capacity::Disabled));assert!(disabled.cap(now).is_none());
        let mut capacity=Capacity::new(true);capacity.probed(now,3_000_000);assert_eq!(capacity.cap(now+Duration::from_secs(1)),Some(3_000_000));assert!(capacity.cap(now+probe::FRESHNESS+Duration::from_nanos(1)).is_none());
    }
}
