//! Opt-in composition of the SDK's public GCC stages. Only Kalman process
//! noise differs (0.1 vs0.001); no threshold, loss, AIMD or rate limit is widened.
//! Independent of the pacing gain. This is local qualification, not a new default.
use super::*;
use rtc::interceptor::{ArrivalGroupAccumulator,Kalman,OveruseDetector,RateCalculator,RateController,LossController,BandwidthEstimator,EstimatorStats,PacketReport};

pub(super) struct FastGcc {
    groups:ArrivalGroupAccumulator,filter:Kalman,detector:OveruseDetector,
    rate:RateCalculator,delay:RateController,loss:LossController,min:f64,max:f64,target:f64,
}
impl FastGcc {
    pub(super) fn new(initial:f64,min:f64,max:f64)->Self {
        Self {groups:ArrivalGroupAccumulator::default(),filter:Kalman::new().with_process_noise(0.1),detector:OveruseDetector::new(),rate:RateCalculator::default(),
            delay:RateController::new(initial,min,max),loss:LossController::new(initial,min,max),min,max,target:initial}
    }
}
impl BandwidthEstimator for FastGcc {
    fn on_reports(&mut self,now:Instant,reports:&[PacketReport]) {
        if reports.is_empty() {return;}
        let mut usage=self.detector.usage();
        for report in reports {
            if report.arrived {self.rate.add(now,report.size);}
            if let Some(group)=self.groups.accumulate(report) {usage=self.detector.update(group.at,self.filter.update(group.delta_ms));}
        }
        let delay=self.delay.update(now,usage,self.rate.rate_bits_per_second(now));
        let loss=self.loss.update(now,reports.iter().filter(|report|!report.arrived).count(),reports.len());
        self.target=delay.min(loss).clamp(self.min,self.max);
    }
    fn target_bitrate(&self)->f64 {self.target}
    fn stats(&self)->EstimatorStats {EstimatorStats {delay_based_bitrate:Some(self.delay.target()),loss_based_bitrate:Some(self.loss.target()),packet_loss:self.loss.average_loss(),round_trip_time:None}}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(id:u64,origin:Instant,sent_ms:u64,arrival_ms:u64)->PacketReport {
        PacketReport {ssrc:1,id,rtp_sequence_number:id as u16,is_twcc:true,twcc_sequence_number:id as u16,size:1200,arrived:true,departure:origin+Duration::from_millis(sent_ms),arrival:Some(Duration::from_millis(arrival_ms)),ecn:Default::default()}
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
