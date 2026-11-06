//! Fixed-size accounting for an optional pre-encode producer gate. Packet
//! pacing may burst; an idle producer never accumulates burst credit.
use std::time::{Duration,Instant};

/// Local experiment. The embedder must gate raw input before encoding; dropping
/// inter-predicted encoded pictures to repay debt would break the reference chain.
#[derive(Debug,Clone,Copy,Default)]
pub enum ProducerControl {#[default] Unbounded,RtpDebt}
/// Current-epoch cumulative serialized RTP plus an explicit 64-byte/packet
/// reserve. This is not full wire traffic: RTCP, ICE, TURN/TLS and provider
/// billing are not measured here. A held media rate is not measured capacity.
#[derive(Debug,Clone,Copy)]
pub struct ProducerBudget {pub wait:Duration,pub rtp_packets:u64,pub rtp_bytes:u64,pub reserved_bytes:u64}
const RESERVE:u64=64;
const MAX_WAIT:Duration=Duration::from_secs(5);
pub(super) enum Ledger {Disabled,Tracking(Account)}
pub(super) struct Account {at:Instant,rate:u32,nanobits:u128,packets:u64,bytes:u64,reserved:u64}
impl Ledger {
    pub(super) fn new(mode:ProducerControl,now:Instant,rate:u32)->Self {
        match mode {ProducerControl::Unbounded=>Self::Disabled,ProducerControl::RtpDebt=>Self::Tracking(Account {at:now,rate,nanobits:0,packets:0,bytes:0,reserved:0})}
    }
    pub(super) fn reprice(&mut self,now:Instant,rate:u32)->Result<(),()> {
        if let Self::Tracking(account)=self {account.settle(now);account.rate=rate;account.wait()?;}Ok(())
    }
    pub(super) fn charge(&mut self,now:Instant,bytes:usize)->Result<(),()> {
        if let Self::Tracking(account)=self {
            account.settle(now);
            let bytes=u64::try_from(bytes).map_err(|_|())?;
            account.packets=account.packets.checked_add(1).ok_or(())?;
            account.bytes=account.bytes.checked_add(bytes).ok_or(())?;
            account.reserved=account.reserved.checked_add(RESERVE).ok_or(())?;
            let bits=u128::from(bytes.checked_add(RESERVE).ok_or(())?)*8*1_000_000_000;
            account.nanobits=account.nanobits.checked_add(bits).ok_or(())?;account.wait()?;
        }Ok(())
    }
    pub(super) fn snapshot(&mut self,now:Instant)->Result<Option<ProducerBudget>,()> {
        match self {Self::Disabled=>Ok(None),Self::Tracking(account)=>{
            account.settle(now);Ok(Some(ProducerBudget {wait:account.wait()?,rtp_packets:account.packets,rtp_bytes:account.bytes,reserved_bytes:account.reserved}))
        }}
    }
}
impl Account {
    fn settle(&mut self,now:Instant) {
        // A worker snapshot can be newer than the driver's release timestamp.
        // Never move the clock backwards or repay the same time twice.
        if now>self.at {self.nanobits=self.nanobits.saturating_sub(u128::from(self.rate)*now.duration_since(self.at).as_nanos());self.at=now;}
    }
    fn wait(&self)->Result<Duration,()> {
        if self.rate==0 {return Err(());}
        let nanos=self.nanobits.div_ceil(u128::from(self.rate));
        if nanos>MAX_WAIT.as_nanos() {return Err(());}
        Ok(Duration::from_nanos(u64::try_from(nanos).map_err(|_|())?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget(ledger:&mut Ledger,now:Instant)->ProducerBudget {ledger.snapshot(now).unwrap().unwrap()}
    #[test]
    fn producer_debt_many_small_packets_exceed_a_single_frame_ceiling() {
        let now=Instant::now();let mut ledger=Ledger::new(ProducerControl::RtpDebt,now,1_000_000);
        for _ in 0..100 {ledger.charge(now,1000).unwrap();}
        let value=budget(&mut ledger,now);assert_eq!(value.rtp_packets,100);assert_eq!(value.rtp_bytes,100_000);assert_eq!(value.reserved_bytes,6400);assert_eq!(value.wait,Duration::from_micros(851_200));
        assert_eq!(budget(&mut ledger,now+value.wait).wait,Duration::ZERO);
    }
    #[test]
    fn producer_debt_repricing_repays_old_rate_without_forgiving_existing_work() {
        let now=Instant::now();let mut ledger=Ledger::new(ProducerControl::RtpDebt,now,1_000_000);ledger.charge(now,124_936).unwrap();
        ledger.reprice(now+Duration::from_millis(250),500_000).unwrap();
        assert_eq!(budget(&mut ledger,now+Duration::from_millis(250)).wait,Duration::from_millis(1500));
        assert_eq!(budget(&mut ledger,now+Duration::from_millis(750)).wait,Duration::from_secs(1));
    }
    #[test]
    fn producer_debt_idle_and_out_of_order_release_never_mint_credit() {
        let now=Instant::now();let mut ledger=Ledger::new(ProducerControl::RtpDebt,now,1_000_000);
        assert_eq!(budget(&mut ledger,now+Duration::from_secs(10)).wait,Duration::ZERO);
        ledger.charge(now+Duration::from_secs(9),1186).unwrap();
        assert_eq!(budget(&mut ledger,now+Duration::from_secs(10)).wait,Duration::from_millis(10));
        assert_eq!(budget(&mut ledger,now+Duration::from_secs(10)).wait,Duration::from_millis(10));
    }
    #[test]
    fn producer_debt_disabled_is_inert_and_overflow_or_excessive_pause_fails() {
        let now=Instant::now();let mut disabled=Ledger::new(ProducerControl::Unbounded,now,0);disabled.charge(now,usize::MAX).unwrap();assert!(disabled.snapshot(now).unwrap().is_none());
        let mut ledger=Ledger::new(ProducerControl::RtpDebt,now,250_000);assert!(ledger.charge(now,156_187).is_err());
        let mut ledger=Ledger::new(ProducerControl::RtpDebt,now,1_000_000);if let Ledger::Tracking(account)=&mut ledger {account.packets=u64::MAX;}assert!(ledger.charge(now,1).is_err());
        let mut ledger=Ledger::new(ProducerControl::RtpDebt,now,1_000_000);ledger.charge(now,200_000).unwrap();assert!(ledger.reprice(now,250_000).is_err());
    }
}
