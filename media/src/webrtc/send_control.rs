//! Optional GCC and bounded pacing. The serial producer waits for release, not
//! just driver enqueue. Queue loss is terminal; close retires queued RTP.
use super::*;
use rtc::interceptor::{Attribute,BandwidthEstimator,EstimatorStats,Gcc,Interceptor,Packet,PacketReport,PacerBuilder,PacerInterceptor,StreamInfo,TaggedPacket,PACER_MIN_BURST_BITS};
use rtc::sansio::Protocol;
use rtc::shared::error::Error;
use std::collections::{VecDeque,HashMap};

pub(super) const WINDOW:u64=16;
pub(super) const FRAME_DEADLINE:Duration=Duration::from_millis(500);
const MAX_QUEUED:usize=256;
fn burst_bits(rate:f64)->f64 {(rate/200.0).clamp(PACER_MIN_BURST_BITS,48_000.0)}

/// Local qualification limits. The maximum must also fit the viewer's request.
/// Rates limit the media target, not provider-billed or encoded-byte ceilings.
#[derive(Debug,Clone,Copy)]
pub struct SendControl {min:u32,initial:u32,max:u32,gain:PacingGain,delay:DelayControl}
/// Local pacing qualification. Headroom changes packet departure timing, not
/// the encoder's average target. It can still queue on a constrained path.
#[derive(Debug,Clone,Copy,Default)]
pub enum PacingGain {#[default] MediaRate,Responsive}
/// Local estimator qualification; pacing and media ceilings remain independent.
#[derive(Debug,Clone,Copy,Default)]
pub enum DelayControl {#[default] Standard,Trendline}
/// Advisory encoded-byte admission from the selected pacing configuration.
/// Unknown feedback uses the configured prior; a held estimate is not capacity.
#[derive(Debug,Clone,Copy)]
pub struct FrameBudget {pub payload_bytes:u32,pub media_target_bps:u32,pub pacing_target_bps:u32,pub estimate_age:Option<Duration>}
impl SendControl {
    pub fn new(min_kbps:u32,initial_kbps:u32,max_kbps:u32)->Result<Self,WebRtcError> {
        if min_kbps<250 || min_kbps>initial_kbps || initial_kbps>max_kbps || max_kbps>20_000 {return Err(WebRtcError::Configuration);}
        Ok(Self {min:min_kbps*1000,initial:initial_kbps*1000,max:max_kbps*1000,gain:PacingGain::MediaRate,delay:DelayControl::Standard})
    }
    pub fn with_pacing_gain(mut self,gain:PacingGain)->Self {self.gain=gain;self}
    pub fn with_delay_control(mut self,delay:DelayControl)->Self {self.delay=delay;self}
    fn paced_bps(self,media_bps:u32)->u32 {match self.gain {PacingGain::MediaRate=>media_bps,PacingGain::Responsive=>(u64::from(media_bps)*5/2) as u32}}
    fn budget(self,bps:u32,age:Option<Duration>)->FrameBudget {
        let paced=self.paced_bps(bps);
        // 350 ms of pacing, reserving ten percent for packet overhead and
        // coalesced estimator changes. The send deadline stays at 500 ms.
        FrameBudget {payload_bytes:(u64::from(paced)*315/8000) as u32,media_target_bps:bps,pacing_target_bps:paced,estimate_age:age}
    }
    pub fn initial_frame_budget(self)->FrameBudget {self.budget(self.initial,None)}
}
#[derive(Debug,Clone,Copy)]
pub struct BitrateEstimate {pub target_kbps:u32,pub age:Duration}
#[derive(Clone,Copy)]
enum Rate {Unknown,Observed {bps:u32,at:Instant,revision:u64}}
struct Epoch {id:u64,started:Instant,rate:Rate}
#[repr(u8)]
#[derive(Clone,Copy)]
enum Health {Active,Failed}

pub(super) struct Control {
    limits:SendControl,authority:Arc<AtomicBool>,changes:Arc<ChangeSignal>,
    health:AtomicU8,issued:AtomicU64,released:AtomicU64,release_target:AtomicU64,epoch_id:AtomicU64,epoch:Mutex<Epoch>,
}
#[derive(Debug,Clone,Copy)]
struct Ticket {ordinal:u64,epoch:u64}
impl Control {
    pub(super) fn new(limits:SendControl,authority:Arc<AtomicBool>,changes:Arc<ChangeSignal>)->Arc<Self> {
        Arc::new(Self {limits,authority,changes,health:AtomicU8::new(Health::Active as u8),issued:AtomicU64::new(0),released:AtomicU64::new(0),release_target:AtomicU64::new(0),epoch_id:AtomicU64::new(0),epoch:Mutex::new(Epoch {id:0,started:Instant::now(),rate:Rate::Unknown})})
    }
    fn active(&self)->bool {!self.authority.load(Ordering::Acquire) && !self.failed()}
    pub(super) fn failed(&self)->bool {self.health.load(Ordering::Acquire)!=Health::Active as u8}
    fn fail(&self) {
        if self.health.swap(Health::Failed as u8,Ordering::AcqRel)==Health::Active as u8 {self.changes.changed();}
    }
    pub(super) fn decorate(&self,message:&mut TaggedPacket)->Result<(),Error> {
        if !self.active() {return Err(Error::Other("desktop pacing ended".into()));}
        if matches!(message.message.packet,Packet::Rtp(_)) && !message.message.has(&Attribute::Retransmission) {
            let mut current=self.issued.load(Ordering::Acquire);
            let ordinal=loop {
                let Some(next)=current.checked_add(1) else {self.fail();return Err(Error::Other("desktop pacing ticket exhausted".into()));};
                match self.issued.compare_exchange_weak(current,next,Ordering::AcqRel,Ordering::Acquire) {
                    Ok(_)=>break next,Err(observed)=>current=observed,
                }
            };
            message.message.add(Attribute::Custom(Arc::new(Ticket {ordinal,epoch:self.epoch_id.load(Ordering::Acquire)})));
        }
        Ok(())
    }
    pub(super) fn retire(&self) {
        let mut epoch=self.epoch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(id)=epoch.id.checked_add(1) else {self.fail();return;};
        *epoch=Epoch {id,started:Instant::now(),rate:Rate::Unknown};
        self.released.store(self.issued.load(Ordering::Acquire),Ordering::Release);
        self.epoch_id.store(id,Ordering::Release);self.changes.changed();
    }
    pub(super) fn estimate(&self)->Option<BitrateEstimate> {
        match self.epoch.lock().unwrap_or_else(std::sync::PoisonError::into_inner).rate {
            Rate::Unknown=>None,
            Rate::Observed {bps,at,..}=>Some(BitrateEstimate {target_kbps:(bps/1000).max(250),age:at.elapsed()}),
        }
    }
    pub(super) fn encoder_target(&self)->Option<u32> {
        let epoch=self.epoch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match epoch.rate {
            Rate::Observed {bps,at,..} if self.active() && at.elapsed()<=Duration::from_secs(2)=>Some(((u64::from(bps)*85/100)/1000).max(250) as u32),
            _=>None,
        }
    }
    pub(super) fn frame_budget(&self)->Option<FrameBudget> {
        let epoch=self.epoch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.active() {return None;}
        Some(match epoch.rate {Rate::Unknown=>self.limits.initial_frame_budget(),Rate::Observed {bps,at,..}=>self.limits.budget(bps,Some(at.elapsed()))})
    }
    pub(super) fn changed_target(&self,seen:&mut (u64,u64))->Option<u32> {
        let epoch=self.epoch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match epoch.rate {
            Rate::Observed {bps,at,revision} if (epoch.id,revision)!=*seen && at.elapsed()<=Duration::from_secs(2)=> {
                *seen=(epoch.id,revision);Some(((u64::from(bps)*85/100)/1000).max(250) as u32)
            },
            _=>None,
        }
    }
    pub(super) async fn wait_released(&self,ticket:u64)->Result<(),WebRtcError> {
        // One serial producer: only its chunk/frame boundary needs a wake.
        // Earlier packet releases advance the counter without waking its task.
        self.release_target.store(ticket,Ordering::Release);
        loop {
            let revision=self.changes.revision();
            if !self.active() {return Err(WebRtcError::Ended);}
            if self.released.load(Ordering::Acquire)>=ticket {self.release_target.store(0,Ordering::Release);return Ok(());}
            self.changes.wait(revision).await;
        }
    }
}

#[cfg(test)]
struct GccTrace {slope:rtc::interceptor::SlopeEstimator,detector:rtc::interceptor::OveruseDetector,origin:Instant,left:u16}
#[cfg(test)]
impl GccTrace {
    fn new()->Self {Self {slope:rtc::interceptor::SlopeEstimator::new(),detector:rtc::interceptor::OveruseDetector::new(),origin:Instant::now(),left:128}}
    fn record(&mut self,now:Instant,reports:&[PacketReport],gcc:&Engine) {
        let mut last=None;
        for report in reports {if let Some(trend)=self.slope.accumulate(report) {let threshold=self.detector.threshold_ms();let usage=self.detector.update(trend.at,trend.estimate_ms);last=Some((trend,threshold,usage));}}
        if self.left>0 && let Some((trend,threshold,usage))=last {
            self.left-=1;eprintln!("gcc_trace elapsed_us={} reports={} raw_ms={:.3} baseline_filtered_ms={:.3} baseline_threshold_ms={:.3} baseline_usage={usage:?} delay_bps={:.0} loss_bps={:.0}",now.duration_since(self.origin).as_micros(),reports.len(),trend.measurement_ms,trend.estimate_ms,threshold,gcc.stats().delay_based_bitrate.unwrap_or_default(),gcc.stats().loss_based_bitrate.unwrap_or_default());
        }
    }
}
enum Engine {Standard(Gcc),Fast(super::fast_delay::FastGcc)}
impl Engine {
    fn new(limits:SendControl)->Self {let (initial,min,max)=(f64::from(limits.initial),f64::from(limits.min),f64::from(limits.max));match limits.delay {DelayControl::Standard=>Self::Standard(Gcc::new(initial,min,max)),DelayControl::Trendline=>Self::Fast(super::fast_delay::FastGcc::new(initial,min,max))}}
}
impl BandwidthEstimator for Engine {
    fn on_reports(&mut self,now:Instant,reports:&[PacketReport]) {match self {Self::Standard(inner)=>inner.on_reports(now,reports),Self::Fast(inner)=>inner.on_reports(now,reports)}}
    fn target_bitrate(&self)->f64 {match self {Self::Standard(inner)=>inner.target_bitrate(),Self::Fast(inner)=>inner.target_bitrate()}}
    fn stats(&self)->EstimatorStats {match self {Self::Standard(inner)=>inner.stats(),Self::Fast(inner)=>inner.stats()}}
}
pub(super) struct Estimator {inner:Engine,control:Arc<Control>,epoch:u64,#[cfg(test)] trace:Option<GccTrace>}
impl Estimator {
    pub(super) fn new(control:Arc<Control>)->Self {
        let limits=control.limits;Self {inner:Engine::new(limits),control,epoch:0,#[cfg(test)] trace:std::env::var_os("VOXL_TRACE_GCC").map(|_|GccTrace::new())}
    }
}
impl BandwidthEstimator for Estimator {
    fn on_reports(&mut self,now:Instant,reports:&[PacketReport]) {
        let mut epoch=self.control.epoch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.epoch!=epoch.id {
            self.inner=Engine::new(self.control.limits);self.epoch=epoch.id;
            #[cfg(test)]
            if self.trace.is_some() {self.trace=Some(GccTrace::new());}
        }
        let current:Vec<_>=reports.iter().filter(|report|report.departure>=epoch.started && report.departure<=now).cloned().collect();
        if current.is_empty() || !self.control.active() {return;}
        self.inner.on_reports(now,&current);
        #[cfg(test)]
        if let Some(trace)=&mut self.trace {trace.record(now,&current,&self.inner);}
        let target=self.inner.target_bitrate();
        if !target.is_finite() || target<=0.0 {drop(epoch);self.control.fail();return;}
        let bps=(target as u32).clamp(self.control.limits.min,self.control.limits.max);
        let (old,revision)=match epoch.rate {Rate::Unknown=>(0,0),Rate::Observed {bps,revision,..}=>(bps,revision)};
        let changed=old==0 || old.abs_diff(bps)>=old/20;
        let revision=if changed {revision.saturating_add(1)} else {revision};
        epoch.rate=Rate::Observed {bps:if changed {bps} else {old},at:now,revision};drop(epoch);
        if changed {self.control.changes.changed();}
    }
    fn target_bitrate(&self)->f64 {self.inner.target_bitrate()}
    fn stats(&self)->EstimatorStats {self.inner.stats()}
}

#[derive(Clone,Copy,PartialEq,Eq,Hash)]
struct PacketKey {ssrc:u32,sequence:u16,timestamp:u32}
enum Pacing {Running {inner:Box<PacerInterceptor>,pending:VecDeque<Instant>,history:HashMap<PacketKey,Ticket>,order:VecDeque<PacketKey>},Stopped}
pub(super) struct BoundedPacer {state:Pacing,control:Arc<Control>,epoch:u64}
impl BoundedPacer {
    pub(super) fn new(control:Arc<Control>)->Self {
        Self {state:Self::running(control.limits),control,epoch:0}
    }
    fn running(limits:SendControl)->Pacing {
        let paced=f64::from(limits.paced_bps(limits.initial));
        Pacing::Running {inner:Box::new(PacerBuilder::new().with_target_bitrate(paced).with_burst_bits(burst_bits(paced)).with_queue_limit(MAX_QUEUED).build()),pending:VecDeque::new(),history:HashMap::new(),order:VecDeque::new()}
    }
    fn ready(&mut self,now:Option<Instant>)->bool {
        if !self.control.active() {self.state=Pacing::Stopped;return false;}
        let epoch=self.control.epoch_id.load(Ordering::Acquire);
        if self.epoch!=epoch {self.state=Self::running(self.control.limits);self.epoch=epoch;}
        if let Pacing::Running {pending,..}=&self.state
            && let (Some(now),Some(first))=(now,pending.front())
            && now.saturating_duration_since(*first)>=FRAME_DEADLINE {
                self.control.fail();self.state=Pacing::Stopped;return false;
            }
        matches!(self.state,Pacing::Running {..})
    }
}
impl Protocol<TaggedPacket,TaggedPacket,()> for BoundedPacer {
    type Rout=TaggedPacket;type Wout=TaggedPacket;type Eout=();type Error=Error;type Time=Instant;
    fn handle_read(&mut self,message:TaggedPacket)->Result<(),Error> {
        if !self.ready(Some(message.now)) {return Ok(());}
        let rate=match message.message.get(&Attribute::TargetBitrateChanged {bits_per_second:0.0}) {Some(Attribute::TargetBitrateChanged {bits_per_second})=>Some(*bits_per_second),_=>None};
        if let Some(rate)=rate
            && (!rate.is_finite() || rate<f64::from(self.control.limits.min) || rate>f64::from(self.control.limits.max)) {
                self.control.fail();self.state=Pacing::Stopped;return Err(Error::Other("invalid desktop pacing estimate".into()));
            }
        let Pacing::Running {inner,..}=&mut self.state else {return Ok(());};inner.handle_read(message)?;
        if let Some(rate)=rate {
            let paced=f64::from(self.control.limits.paced_bps(rate as u32));
            inner.pacer_mut().set_target_bitrate(paced);
            *inner.pacer_mut()=inner.pacer().clone().with_burst_bits(burst_bits(paced));
        }
        Ok(())
    }
    fn poll_read(&mut self)->Option<TaggedPacket> {
        if !self.ready(None) {return None;}
        let Pacing::Running {inner,..}=&mut self.state else {return None;};inner.poll_read()
    }
    fn handle_write(&mut self,mut message:TaggedPacket)->Result<(),Error> {
        if !self.ready(Some(message.now)) {return Ok(());}
        let rtp=matches!(message.message.packet,Packet::Rtp(_));let now=message.now;
        let Pacing::Running {inner,pending,history,order}=&mut self.state else {return Ok(());};
        if let Packet::Rtp(packet)=&message.message.packet {
            let key=PacketKey {ssrc:packet.header.ssrc,sequence:packet.header.sequence_number,timestamp:packet.header.timestamp};
            if message.message.has(&Attribute::Retransmission) {
                // This SDK buffers raw RTP in its NACK responder, so custom
                // attributes do not survive retransmission. Restore only an
                // exact current-generation identity. No RTX codec is registered.
                let Some(ticket)=history.get(&key) else {return Ok(());};
                message.message.add(Attribute::Custom(Arc::new(*ticket)));
            } else {
                let Some(ticket)=message.message.custom::<Ticket>() else {self.control.fail();self.state=Pacing::Stopped;return Err(Error::Other("desktop pacing ticket missing".into()));};
                if history.insert(key,*ticket).is_none() {order.push_back(key);}
                // Match the imported NACK responder's 1024-packet buffer.
                if order.len()>1024 && let Some(old)=order.pop_front() {history.remove(&old);}
            }
        }
        if rtp && pending.len()>=MAX_QUEUED {self.control.fail();self.state=Pacing::Stopped;return Err(Error::Other("desktop pacing queue full".into()));}
        let dropped=inner.dropped();inner.handle_write(message)?;
        if inner.dropped()!=dropped {self.control.fail();self.state=Pacing::Stopped;return Err(Error::Other("desktop pacing lost a packet".into()));}
        if rtp {pending.push_back(now);}
        Ok(())
    }
    fn poll_write(&mut self)->Option<TaggedPacket> {
        if !self.ready(None) {return None;}
        let Pacing::Running {inner,pending,..}=&mut self.state else {return None;};
        loop {
            let message=inner.poll_write()?;
            if matches!(message.message.packet,Packet::Rtp(_)) {
                if pending.pop_front().is_none() {self.control.fail();self.state=Pacing::Stopped;return None;}
                let Some(ticket)=message.message.custom::<Ticket>() else {self.control.fail();self.state=Pacing::Stopped;return None;};
                // Old NACK-buffer copies may reappear after a restart. Retire
                // them with the old generation, never send them on the new path.
                if ticket.epoch!=self.epoch {continue;}
                if !message.message.has(&Attribute::Retransmission) {
                    if ticket.ordinal!=self.control.released.load(Ordering::Acquire).saturating_add(1) {self.control.fail();self.state=Pacing::Stopped;return None;}
                    self.control.released.store(ticket.ordinal,Ordering::Release);
                    let target=self.control.release_target.load(Ordering::Acquire);
                    if target!=0 && ticket.ordinal>=target
                        && self.control.release_target.compare_exchange(target,0,Ordering::AcqRel,Ordering::Acquire).is_ok() {self.control.changes.changed();}
                }
            }
            return Some(message);
        }
    }
    fn handle_timeout(&mut self,now:Instant)->Result<(),Error> {
        if !self.ready(Some(now)) {return Ok(());}
        let Pacing::Running {inner,..}=&mut self.state else {return Ok(());};inner.handle_timeout(now)
    }
    fn poll_timeout(&mut self)->Option<Instant> {
        if !self.control.active() {return None;}
        match &mut self.state {
            Pacing::Running {inner,pending,..}=>match (inner.poll_timeout(),pending.front().map(|at|*at+FRAME_DEADLINE)) {
                (Some(a),Some(b))=>Some(a.min(b)),(a,b)=>a.or(b),
            },Pacing::Stopped=>None,
        }
    }
}
impl Interceptor for BoundedPacer {
    fn bind_local_stream(&mut self,info:&StreamInfo) {if let Pacing::Running {inner,..}=&mut self.state {inner.bind_local_stream(info);}}
    fn unbind_local_stream(&mut self,info:&StreamInfo) {if let Pacing::Running {inner,..}=&mut self.state {inner.unbind_local_stream(info);}}
    fn bind_remote_stream(&mut self,info:&StreamInfo) {if let Pacing::Running {inner,..}=&mut self.state {inner.bind_remote_stream(info);}}
    fn unbind_remote_stream(&mut self,info:&StreamInfo) {if let Pacing::Running {inner,..}=&mut self.state {inner.unbind_remote_stream(info);}}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn control()->Arc<Control> {Control::new(SendControl::new(250,1000,1000).unwrap(),Arc::new(AtomicBool::new(false)),Arc::new(ChangeSignal::default()))}
    fn packet(control:&Control,sequence:u16,now:Instant)->TaggedPacket {
        let mut message=TaggedPacket {now,transport:Default::default(),message:Packet::Rtp(rtc::rtp::Packet {header:rtc::rtp::header::Header {version:2,ssrc:55,sequence_number:sequence,timestamp:90_000,..Default::default()},payload:Bytes::from(vec![7;1000])}).into()};
        control.decorate(&mut message).unwrap();message
    }
    #[test]
    fn adaptive_limits_reject_invalid_ranges_and_have_no_unobserved_estimate() {
        for limits in [(0,1000,2000),(249,1000,2000),(500,250,2000),(250,2001,2000),(250,1000,20_001)] {assert!(SendControl::new(limits.0,limits.1,limits.2).is_err());}
        assert!(control().estimate().is_none());
    }
    #[test]
    fn adaptive_pacer_has_a_finite_release_schedule_and_close_retires_queued_rtp() {
        let control=control();let mut pacer=BoundedPacer::new(Arc::clone(&control));let now=Instant::now();
        for sequence in 0..10 {pacer.handle_write(packet(&control,sequence,now)).unwrap();}
        pacer.handle_timeout(now).unwrap();let mut sent=0;
        while pacer.poll_write().is_some() {sent+=1;}
        assert_eq!(sent,1);assert_eq!(control.released.load(Ordering::Acquire),1);
        let next=pacer.poll_timeout().unwrap();assert!(next>now && next<=now+Duration::from_millis(10));
        pacer.handle_timeout(next).unwrap();assert!(pacer.poll_write().is_some());
        control.authority.store(true,Ordering::Release);
        assert!(pacer.poll_write().is_none());assert!(pacer.poll_timeout().is_none());
        assert!(matches!(pacer.state,Pacing::Stopped));assert!(!control.failed());
    }
    #[test]
    fn adaptive_queue_overflow_and_deadline_are_terminal_instead_of_silent_loss() {
        for deadline in [false,true] {
            let control=control();let mut pacer=BoundedPacer::new(Arc::clone(&control));let now=Instant::now();
            if deadline {
                pacer.handle_write(packet(&control,0,now)).unwrap();
                pacer.handle_timeout(now+FRAME_DEADLINE).unwrap();
            } else {
                for sequence in 0..MAX_QUEUED {pacer.handle_write(packet(&control,sequence as u16,now)).unwrap();}
                assert!(pacer.handle_write(packet(&control,MAX_QUEUED as u16,now)).is_err());
            }
            assert!(control.failed());assert!(pacer.poll_write().is_none());assert!(pacer.poll_timeout().is_none());
        }
    }
    #[test]
    fn adaptive_producer_release_wake_is_coalesced_at_the_requested_boundary() {
        let control=control();let mut pacer=BoundedPacer::new(Arc::clone(&control));let now=Instant::now();
        control.release_target.store(3,Ordering::Release);
        for sequence in 0..10 {pacer.handle_write(packet(&control,sequence,now)).unwrap();}
        let revision=control.changes.revision();
        while control.released.load(Ordering::Acquire)<10 {
            let deadline=pacer.poll_timeout().unwrap();pacer.handle_timeout(deadline).unwrap();
            while pacer.poll_write().is_some() {}
            let released=control.released.load(Ordering::Acquire);
            assert_eq!(control.changes.revision(),revision+u64::from(released>=3));
        }
        assert_eq!(control.release_target.load(Ordering::Acquire),0);
    }
    #[test]
    fn adaptive_retransmissions_keep_the_current_epoch_without_false_producer_completion() {
        let control=control();let mut pacer=BoundedPacer::new(Arc::clone(&control));let now=Instant::now();
        let original=packet(&control,u16::MAX,now);let raw=original.message.packet.clone();
        pacer.handle_write(original).unwrap();pacer.handle_timeout(now).unwrap();assert!(pacer.poll_write().is_some());
        let retransmit=||TaggedPacket {now,transport:Default::default(),message:rtc::interceptor::AttributedPacket::new(raw.clone()).with(Attribute::Retransmission)};
        pacer.handle_write(retransmit()).unwrap();
        let retransmission_deadline=pacer.poll_timeout().unwrap();assert!(retransmission_deadline>now);
        pacer.handle_timeout(retransmission_deadline).unwrap();assert!(pacer.poll_write().is_some());
        assert_eq!(control.released.load(Ordering::Acquire),1);
        control.retire();pacer.handle_write(retransmit()).unwrap();pacer.handle_timeout(now).unwrap();assert!(pacer.poll_write().is_none());
        pacer.handle_write(packet(&control,0,now)).unwrap();pacer.handle_timeout(now).unwrap();assert!(pacer.poll_write().is_some());
        assert_eq!(control.released.load(Ordering::Acquire),2);assert!(!control.failed());
    }
    #[test]
    fn adaptive_feedback_retires_old_path_reports_and_coalesces_encoder_targets() {
        let control=control();let mut estimator=Estimator::new(Arc::clone(&control));let now=Instant::now();
        estimator.on_reports(now,&[]);assert!(control.estimate().is_none());
        let report=PacketReport {ssrc:55,id:1,rtp_sequence_number:1,is_twcc:true,twcc_sequence_number:1,size:1000,arrived:true,departure:now,arrival:Some(Duration::from_millis(10)),ecn:Default::default()};
        estimator.on_reports(now+Duration::from_millis(1),std::slice::from_ref(&report));
        let mut seen=(0,0);let target=control.changed_target(&mut seen).unwrap();assert!((250..=850).contains(&target));
        assert!(control.changed_target(&mut seen).is_none());
        control.retire();estimator.on_reports(now+Duration::from_millis(2),&[report]);assert!(control.estimate().is_none());
        let mut epoch=control.epoch.lock().unwrap();epoch.rate=Rate::Observed {bps:500_000,at:Instant::now(),revision:1};drop(epoch);
        assert_eq!(control.changed_target(&mut seen),Some(425));
    }
    #[test]
    fn adaptive_worker_observation_rejects_unknown_stale_retired_and_closed_rates() {
        let control=control();assert!(control.encoder_target().is_none());
        control.epoch.lock().unwrap().rate=Rate::Observed {bps:500_000,at:Instant::now(),revision:1};
        assert_eq!(control.encoder_target(),Some(425));assert_eq!(control.encoder_target(),Some(425));
        control.epoch.lock().unwrap().rate=Rate::Observed {bps:500_000,at:Instant::now()-Duration::from_secs(3),revision:2};
        assert!(control.encoder_target().is_none());control.retire();assert!(control.encoder_target().is_none());
        control.epoch.lock().unwrap().rate=Rate::Observed {bps:700_000,at:Instant::now(),revision:1};
        assert_eq!(control.encoder_target(),Some(595));control.authority.store(true,Ordering::Release);
        assert!(control.encoder_target().is_none());
    }
    #[test]
    fn adaptive_responsive_pacing_drains_the_known_keyframe_burst_within_the_existing_deadline() {
        for gain in [PacingGain::MediaRate,PacingGain::Responsive] {
            let limits=SendControl::new(250,1500,1500).unwrap().with_pacing_gain(gain);
            let control=Control::new(limits,Arc::new(AtomicBool::new(false)),Arc::new(ChangeSignal::default()));
            let mut pacer=BoundedPacer::new(Arc::clone(&control));let start=Instant::now();let mut sent=0;let mut last=start;
            for sequence in 0..100 {pacer.handle_write(packet(&control,sequence,start)).unwrap();}
            for _ in 0..200 {
                // The SDK rounds sub-nanosecond token deficits to zero. A real
                // wake advances the clock; the virtual schedule must do so too.
                let Some(next)=pacer.poll_timeout() else {break;};last=next.max(last+Duration::from_nanos(1));pacer.handle_timeout(last).unwrap();
                while pacer.poll_write().is_some() {sent+=1;}
                if sent==100 {break;}
            }
            match gain {
                PacingGain::MediaRate=>{assert!(sent<100 && control.failed());},
                PacingGain::Responsive=>{assert_eq!(sent,100);assert!(!control.failed());assert!(last.duration_since(start)<Duration::from_millis(300));},
            }
        }
    }
    #[test]
    fn adaptive_responsive_rate_updates_preserve_credit_media_feedback_and_epoch_retirement() {
        let limits=SendControl::new(250,1500,1500).unwrap().with_pacing_gain(PacingGain::Responsive);
        let control=Control::new(limits,Arc::new(AtomicBool::new(false)),Arc::new(ChangeSignal::default()));
        let mut pacer=BoundedPacer::new(Arc::clone(&control));let now=Instant::now();
        for sequence in 0..2 {pacer.handle_write(packet(&control,sequence,now)).unwrap();}
        pacer.handle_timeout(now).unwrap();while pacer.poll_write().is_some() {}
        let update=TaggedPacket {now,transport:Default::default(),message:rtc::interceptor::AttributedPacket::new(Packet::Rtcp(vec![])).with(Attribute::TargetBitrateChanged {bits_per_second:500_000.0})};
        pacer.handle_read(update).unwrap();
        let Pacing::Running {inner,..}=&pacer.state else {panic!("active responsive pacer");};
        assert_eq!(inner.pacer().target_bitrate(),1_250_000.0);assert!(!inner.pacer().can_afford(8000.0));
        let forwarded=pacer.poll_read().unwrap();assert!(matches!(forwarded.message.get(&Attribute::TargetBitrateChanged {bits_per_second:0.0}),Some(Attribute::TargetBitrateChanged {bits_per_second}) if *bits_per_second==500_000.0));
        control.epoch.lock().unwrap().rate=Rate::Observed {bps:500_000,at:now,revision:1};
        assert_eq!(control.encoder_target(),Some(425));let budget=control.frame_budget().unwrap();assert_eq!(budget.media_target_bps,500_000);assert_eq!(budget.pacing_target_bps,1_250_000);
        control.retire();assert!(control.frame_budget().unwrap().estimate_age.is_none());assert!(pacer.ready(None));
        let Pacing::Running {inner,..}=&pacer.state else {panic!("new responsive epoch");};assert_eq!(inner.pacer().target_bitrate(),3_750_000.0);
        control.authority.store(true,Ordering::Release);assert!(control.frame_budget().is_none());
    }
    #[test]
    fn adaptive_frame_budget_is_finite_for_all_supported_media_rates_and_gains() {
        for rate in [250,1500,20_000] {for gain in [PacingGain::MediaRate,PacingGain::Responsive] {
            let limits=SendControl::new(250,rate,rate).unwrap().with_pacing_gain(gain);let budget=limits.initial_frame_budget();
            assert_eq!(budget.media_target_bps,rate*1000);assert!(budget.payload_bytes>0 && budget.payload_bytes as usize<=crate::MAX_FRAME_BYTES);assert!(budget.estimate_age.is_none());
            let paced=f64::from(budget.pacing_target_bps);assert!(f64::from(budget.payload_bytes)*8.0/paced<0.35);
        }}
    }

}
