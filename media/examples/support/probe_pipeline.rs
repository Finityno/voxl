//! Bounded single-producer bookkeeping for the local encoder experiment.
//! Encoded output is never discarded; unexpected/misordered output is terminal.
use std::{collections::VecDeque,time::{Duration,Instant}};

const OUTPUT_DEADLINE:Duration=Duration::from_secs(2);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Submission {pub index:usize,pub pts:i64,pub forced:bool,pub submitted:Instant}
#[derive(Debug,PartialEq,Eq)]
pub(super) enum Action {Submit,Sleep(Duration),Receive(Duration)}
pub(super) struct Pipeline {depth:usize,pending:VecDeque<Submission>,submitted:usize,completed:usize,last_pts:Option<i64>,maximum:usize}
impl Pipeline {
    pub fn new(depth:usize)->Result<Self,&'static str> {
        if !(1..=2).contains(&depth) {return Err("pipeline depth must be 1 or 2");}
        Ok(Self {depth,pending:VecDeque::with_capacity(depth),submitted:0,completed:0,last_pts:None,maximum:0})
    }
    pub fn empty(&self)->bool {self.pending.is_empty()}
    pub fn maximum(&self)->usize {self.maximum}
    pub fn submitted(&self)->usize {self.submitted}
    pub fn completed(&self)->usize {self.completed}
    pub fn submit(&mut self,entry:Submission)->Result<(),&'static str> {
        if self.pending.len()>=self.depth || entry.index!=self.submitted || entry.pts<0 || self.last_pts.is_some_and(|pts|entry.pts<=pts) {
            return Err("pipeline capacity, input order or clock violated");
        }
        self.last_pts=Some(entry.pts);self.submitted+=1;self.pending.push_back(entry);
        self.maximum=self.maximum.max(self.pending.len());Ok(())
    }
    pub fn receive_wait(&self,now:Instant)->Result<Duration,&'static str> {
        let entry=self.pending.front().ok_or("unexpected encoder output")?;
        (entry.submitted+OUTPUT_DEADLINE).checked_duration_since(now).filter(|wait|!wait.is_zero()).ok_or("encoder output deadline")
    }
    pub fn action(&self,now:Instant,next:Instant)->Result<Action,&'static str> {
        let output=if self.empty() {None} else {Some(self.receive_wait(now)?)};
        if self.pending.len()<self.depth {
            let Some(wait)=next.checked_duration_since(now).filter(|wait|!wait.is_zero()) else {return Ok(Action::Submit);};
            Ok(match output {Some(output)=>Action::Receive(wait.min(output)),None=>Action::Sleep(wait)})
        } else {Ok(Action::Receive(output.ok_or("full pipeline has no output deadline")?))}
    }
    pub fn complete(&mut self,pts:i64,now:Instant)->Result<Submission,&'static str> {
        self.receive_wait(now)?;
        let entry=self.pending.front().ok_or("unexpected encoder output")?;
        if entry.pts!=pts || entry.index!=self.completed {return Err("pipeline output order or timestamp mismatch");}
        let entry=self.pending.pop_front().ok_or("unexpected encoder output")?;self.completed+=1;Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(index:usize,now:Instant)->Submission {Submission {index,pts:index as i64*16666,forced:index==0,submitted:now}}
    #[test]
    fn probe_pipeline_bounds_credits_and_preserves_fifo_and_final_drain() {
        assert!(Pipeline::new(0).is_err());assert!(Pipeline::new(3).is_err());let now=Instant::now();
        let mut pipeline=Pipeline::new(2).unwrap();pipeline.submit(frame(0,now)).unwrap();pipeline.submit(frame(1,now)).unwrap();
        assert!(pipeline.submit(frame(2,now)).is_err());assert_eq!(pipeline.maximum(),2);
        assert_eq!(pipeline.complete(0,now).unwrap(),frame(0,now));pipeline.submit(frame(2,now)).unwrap();
        assert_eq!(pipeline.complete(16666,now).unwrap().index,1);assert_eq!(pipeline.complete(33332,now).unwrap().index,2);
        assert!(pipeline.empty());assert_eq!((pipeline.submitted(),pipeline.completed()),(3,3));assert!(pipeline.complete(33332,now).is_err());
    }
    #[test]
    fn probe_pipeline_rejects_bad_clocks_and_reordering_without_consuming_output() {
        let now=Instant::now();let mut pipeline=Pipeline::new(2).unwrap();
        assert!(pipeline.submit(frame(1,now)).is_err());let mut negative=frame(0,now);negative.pts=-1;assert!(pipeline.submit(negative).is_err());
        pipeline.submit(frame(0,now)).unwrap();let mut duplicate=frame(1,now);duplicate.pts=0;assert!(pipeline.submit(duplicate).is_err());
        pipeline.submit(frame(1,now)).unwrap();assert!(pipeline.complete(16666,now).is_err());assert_eq!(pipeline.completed(),0);
        pipeline.complete(0,now).unwrap();assert!(pipeline.complete(0,now).is_err());pipeline.complete(16666,now).unwrap();assert!(pipeline.empty());
    }
    #[test]
    fn probe_pipeline_waits_for_the_earliest_deadline_without_polling_or_extra_credit() {
        let now=Instant::now();let next=now+Duration::from_millis(16);let mut pipeline=Pipeline::new(2).unwrap();
        assert_eq!(pipeline.action(now,next).unwrap(),Action::Sleep(Duration::from_millis(16)));pipeline.submit(frame(0,now)).unwrap();
        assert_eq!(pipeline.action(now,next).unwrap(),Action::Receive(Duration::from_millis(16)));
        assert_eq!(pipeline.action(next,next).unwrap(),Action::Submit);pipeline.submit(frame(1,next)).unwrap();
        assert_eq!(pipeline.action(next,next).unwrap(),Action::Receive(OUTPUT_DEADLINE-Duration::from_millis(16)));
        assert!(pipeline.action(now+OUTPUT_DEADLINE,next).is_err());assert!(pipeline.complete(0,now+OUTPUT_DEADLINE).is_err());
        assert_eq!(pipeline.completed(),0);
    }
    #[test]
    fn probe_pipeline_default_serial_mode_never_accepts_a_second_pending_frame() {
        let now=Instant::now();let mut pipeline=Pipeline::new(1).unwrap();pipeline.submit(frame(0,now)).unwrap();
        assert_eq!(pipeline.action(now,now).unwrap(),Action::Receive(OUTPUT_DEADLINE));assert!(pipeline.submit(frame(1,now)).is_err());
        pipeline.complete(0,now).unwrap();assert_eq!(pipeline.action(now,now).unwrap(),Action::Submit);assert_eq!(pipeline.maximum(),1);
    }
}
