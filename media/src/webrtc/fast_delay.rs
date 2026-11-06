//! Opt-in accumulated-delay regression over a fixed10-group window. The
//! grouping, overuse hysteresis, AIMD, loss and rate limits use SDK components.
//! This is an independent composition inspired by WebRTC's trendline signal,
//! not an implementation of its complete controller or a new default.
use super::*;
use rtc::interceptor::{ArrivalGroupAccumulator,InterGroupDelay,OveruseDetector,RateCalculator,RateController,LossController,BandwidthEstimator,EstimatorStats,PacketReport};

const TREND_WINDOW:usize=10;
#[derive(Default)]
enum TrendPhase {#[default] Empty,Tracking {at:Instant,arrival_ms:f64,accumulated:f64,smoothed:f64,count:u16,used:usize,next:usize}}
enum TrendUpdate {Pending,Reset,Value(f64)}
#[derive(Default)]
struct Trendline {phase:TrendPhase,samples:[(f64,f64);TREND_WINDOW]}
impl Trendline {
    fn update(&mut self,group:InterGroupDelay)->TrendUpdate {
        let TrendPhase::Tracking {at,arrival_ms,accumulated,smoothed,count,used,next}=&mut self.phase else {
            // Fixed scratch storage survives reset; the phase owns which rows
            // are valid, so an old generation contributes no measurement.
            self.phase=TrendPhase::Tracking {at:group.at,arrival_ms:0.0,accumulated:0.0,smoothed:0.0,count:0,used:0,next:0};return TrendUpdate::Reset;
        };
        if group.at<=*at {return TrendUpdate::Pending;}
        let departure_ms=group.at.duration_since(*at).as_secs_f64()*1000.0;let delta=departure_ms+group.delta_ms;
        if !delta.is_finite() || !(0.0..=2000.0).contains(&delta) {self.phase=TrendPhase::Empty;return TrendUpdate::Reset;}
        *at=group.at;*arrival_ms+=delta;*accumulated+=group.delta_ms;*smoothed=0.9* *smoothed+0.1* *accumulated;*count=count.saturating_add(1).min(60);
        self.samples[*next]=(*arrival_ms,*smoothed);*next=(*next+1)%TREND_WINDOW;*used=(*used+1).min(TREND_WINDOW);
        if *used<TREND_WINDOW {return TrendUpdate::Pending;}
        let (x,y)=self.samples.iter().fold((0.0,0.0),|(x,y),(a,b)|(x+a,y+b));let x=x/TREND_WINDOW as f64;let y=y/TREND_WINDOW as f64;
        let (covariance,variance)=self.samples.iter().fold((0.0,0.0),|(cov,var),(a,b)|(cov+(a-x)*(b-y),var+(a-x)*(a-x)));
        if variance>0.0 {TrendUpdate::Value(covariance/variance*f64::from(*count)*4.0)} else {TrendUpdate::Pending}
    }
}
#[derive(Default)]
enum ArrivalClock {#[default] Empty,Running {remote:Duration,origin:Instant,last:Instant}}
#[derive(Default)]
struct ReceiverRate {clock:ArrivalClock,rate:RateCalculator}
impl ReceiverRate {
    fn add(&mut self,now:Instant,arrival:Duration,bytes:usize) {
        if matches!(self.clock,ArrivalClock::Empty) {self.clock=ArrivalClock::Running {remote:arrival,origin:now,last:now};}
        let ArrivalClock::Running {remote,origin,last}=&mut self.clock else {return;};
        // This is a synthetic interval clock only. No sender/receiver absolute
        // clocks are subtracted, and feedback batching cannot compress a rate.
        let Some(mapped)=arrival.checked_sub(*remote).and_then(|elapsed|origin.checked_add(elapsed)) else {return;};
        if mapped<*last {return;}*last=mapped;self.rate.add(mapped,bytes);
    }
    fn bitrate(&mut self)->Option<f64> {match self.clock {ArrivalClock::Empty=>None,ArrivalClock::Running {last,..}=>self.rate.rate_bits_per_second(last)}}
}

pub(super) struct FastGcc {
    groups:ArrivalGroupAccumulator,filter:Trendline,detector:OveruseDetector,
    rate:ReceiverRate,delay:RateController,loss:LossController,min:f64,max:f64,target:f64,
}
impl FastGcc {
    pub(super) fn new(initial:f64,min:f64,max:f64)->Self {
        Self {groups:ArrivalGroupAccumulator::default(),filter:Trendline::default(),detector:OveruseDetector::new(),rate:ReceiverRate::default(),
            delay:RateController::new(initial,min,max),loss:LossController::new(initial,min,max),min,max,target:initial}
    }
}
impl BandwidthEstimator for FastGcc {
    fn on_reports(&mut self,now:Instant,reports:&[PacketReport]) {
        if reports.is_empty() {return;}
        let mut usage=self.detector.usage();
        for report in reports {
            if report.arrived && let Some(arrival)=report.arrival {self.rate.add(now,arrival,report.size);}
            if let Some(group)=self.groups.accumulate(report) {match self.filter.update(group) {
                TrendUpdate::Value(signal)=>usage=self.detector.update(group.at,signal),
                TrendUpdate::Reset=>{self.detector=OveruseDetector::new();usage=self.detector.usage();},TrendUpdate::Pending=>{},
            }}
        }
        let delay=self.delay.update(now,usage,self.rate.bitrate());
        let loss=self.loss.update(now,reports.iter().filter(|report|!report.arrived).count(),reports.len());
        self.target=delay.min(loss).clamp(self.min,self.max);
    }
    fn target_bitrate(&self)->f64 {self.target}
    fn stats(&self)->EstimatorStats {let mut stats=EstimatorStats::default();stats.delay_based_bitrate=Some(self.delay.target());stats.loss_based_bitrate=Some(self.loss.target());stats.packet_loss=self.loss.average_loss();stats}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(id:u64,origin:Instant,sent_ms:u64,arrival_ms:u64)->PacketReport {
        PacketReport {ssrc:1,id,rtp_sequence_number:id as u16,is_twcc:true,twcc_sequence_number:id as u16,size:1200,arrived:true,departure:origin+Duration::from_millis(sent_ms),arrival:Some(Duration::from_millis(arrival_ms)),ecn:Default::default()}
    }
    #[test]
    fn adaptive_fast_delay_gap_retires_old_rows_before_any_new_signal() {
        let origin=Instant::now();let mut trend=Trendline::default();
        for i in 0..80 {let _=trend.update(InterGroupDelay {at:origin+Duration::from_millis(i*10),delta_ms:5.0,size:1200});}
        assert!(matches!(trend.update(InterGroupDelay {at:origin+Duration::from_secs(5),delta_ms:0.0,size:1200}),TrendUpdate::Reset));
        let mut last=0.0;
        for i in 0..30 {if let TrendUpdate::Value(value)=trend.update(InterGroupDelay {at:origin+Duration::from_millis(5010+i*10),delta_ms:0.0,size:1200}) {last=value;}}
        assert_eq!(last,0.0);
    }
    #[test]
    fn adaptive_fast_delay_rate_uses_receiver_intervals_and_ignores_old_samples() {
        let now=Instant::now();let mut rate=ReceiverRate::default();assert!(rate.bitrate().is_none());
        for i in 0..10 {rate.add(now,Duration::from_millis(100+i*10),1200);}
        let measured=rate.bitrate().unwrap();assert!((900_000.0..=1_200_000.0).contains(&measured));
        rate.add(now,Duration::from_millis(1),9999);assert_eq!(rate.bitrate(),Some(measured));
    }
    #[test]
    fn adaptive_fast_delay_tracks_sustained_queue_growth_within_the_same_rate_limits() {
        let origin=Instant::now();let mut fast=FastGcc::new(1_500_000.0,250_000.0,1_500_000.0);let mut reduced=None;
        for id in 0..100 {
            let arrival=100+if id<40 {id*10} else {400+(id-40)*30};let now=origin+Duration::from_millis(arrival+20);
            fast.on_reports(now,&[report(id,origin,id*10,arrival)]);assert!((250_000.0..=1_500_000.0).contains(&fast.target_bitrate()));
            if id>=40 && fast.target_bitrate()<1_000_000.0 && reduced.is_none() {reduced=Some(arrival-500);}
        }
        assert!(reduced.is_some_and(|ms|ms<=300));
    }
    #[test]
    fn adaptive_fast_delay_does_not_back_off_for_alternating_jitter_or_one_delay_spike() {
        for spike in [false,true] {
            let origin=Instant::now();let mut fast=FastGcc::new(1_500_000.0,250_000.0,1_500_000.0);
            for id in 0..200 {let offset=if spike {if id==100 {80} else {0}} else if id%2==0 {10} else {0};let arrival=100+id*100+offset;
                fast.on_reports(origin+Duration::from_millis(arrival+20),&[report(id,origin,id*100,arrival)]);
                assert_eq!(fast.target_bitrate(),1_500_000.0,"transient jitter is not sustained congestion");
            }
        }
    }
    #[test]
    fn adaptive_fast_delay_empty_feedback_does_not_create_a_target_and_loss_still_backs_off() {
        let origin=Instant::now();let mut fast=FastGcc::new(1_500_000.0,250_000.0,1_500_000.0);fast.on_reports(origin,&[]);assert_eq!(fast.target_bitrate(),1_500_000.0);
        for batch in 0..20 {let reports:Vec<_>=(0..10).map(|i| {let mut p=report(batch*10+i,origin,batch*100+i,batch*100+i+20);if i%2==0 {p.arrived=false;p.arrival=None;}p}).collect();fast.on_reports(origin+Duration::from_millis(batch*100+50),&reports);}
        assert!(fast.target_bitrate()<1_000_000.0 && fast.target_bitrate()>=250_000.0);
    }
}
