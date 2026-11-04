//! Bounded trickle ICE. The frame writer alone applies remote candidates after
//! accepting the answer; independent signaling can safely arrive before it.
use super::{Events,VideoCloseHandle,WebRtcError};
use std::{collections::VecDeque,sync::{Arc,Mutex,atomic::{AtomicU64,Ordering}}};
use webrtc::peer_connection::RTCIceCandidateInit;

const MAX_CANDIDATES:usize=64;
const MAX_CANDIDATE_BYTES:usize=8192;

/// Carries addresses inside the application's authenticated signaling channel.
/// Intentionally not Debug: candidate addresses must not enter diagnostics.
#[derive(Clone,PartialEq,Eq)]
pub enum VideoIceSignal {Candidate {candidate:String,mid:String},GatheringComplete}

#[derive(Default)]
struct Queue {pending:VecDeque<(String,String)>,received:usize}
#[derive(Default)]
pub(super) struct Signals {local:Mutex<Queue>,remote:Mutex<Queue>,remote_revision:AtomicU64}
fn bounded(candidate:&str,mid:&str)->bool {
    !candidate.is_empty() && candidate.len()<=MAX_CANDIDATE_BYTES && mid.len()<=256
        && !candidate.chars().any(char::is_control) && !mid.chars().any(char::is_control)
}
fn push(queue:&Mutex<Queue>,candidate:String,mid:String)->Result<(),WebRtcError> {
    if !bounded(&candidate,&mid) {return Err(WebRtcError::Configuration);}
    let mut queue=queue.lock().unwrap_or_else(|error|error.into_inner());
    if queue.received>=MAX_CANDIDATES {return Err(WebRtcError::Configuration);}
    queue.received+=1;queue.pending.push_back((candidate,mid));Ok(())
}
impl Signals {
    pub fn local(&self,candidate:String,mid:String)->Result<(),WebRtcError> {push(&self.local,candidate,mid)}
    pub fn remote_revision(&self)->u64 {self.remote_revision.load(Ordering::Acquire)}
    #[cfg(test)]
    pub fn local_pending(&self)->usize {self.local.lock().unwrap().pending.len()}
    pub fn take_remote(&self)->VecDeque<(String,String)> {
        std::mem::take(&mut self.remote.lock().unwrap_or_else(|error|error.into_inner()).pending)
    }
}

/// Cloneable signaling endpoint with no frame buffers or concurrent frame
/// writer. One consumer long-polls local signals; incoming candidates enqueue
/// synchronously. Closing the view wakes waiters and rejects stale work.
#[derive(Clone)]
pub struct VideoSignaling {pub(super) events:Arc<Events>,pub(super) closer:VideoCloseHandle}
impl VideoSignaling {
    pub async fn next_signal(&self)->Result<VideoIceSignal,WebRtcError> {
        loop {
            let revision=self.events.changes.revision();
            if self.closer.closed.load(Ordering::Acquire) || self.events.ended.load(Ordering::Acquire) {return Err(WebRtcError::Ended);}
            let signals=self.events.signals.as_ref().ok_or(WebRtcError::Configuration)?;
            let next=signals.local.lock().unwrap_or_else(|error|error.into_inner()).pending.pop_front();
            if let Some((candidate,mid))=next {
                let mid=if mid.is_empty() {self.events.media_mid.get().ok_or(WebRtcError::Configuration)?.clone()} else {mid};
                return Ok(VideoIceSignal::Candidate {candidate,mid});
            }
            if self.events.gathered.load(Ordering::Acquire) {return Ok(VideoIceSignal::GatheringComplete);}
            self.events.changes.wait(revision).await;
        }
    }
    pub fn add_candidate(&self,candidate:String,mid:String)->Result<(),WebRtcError> {
        if self.closer.closed.load(Ordering::Acquire) || self.events.ended.load(Ordering::Acquire) {return Err(WebRtcError::Ended);}
        let signals=self.events.signals.as_ref().ok_or(WebRtcError::Configuration)?;
        if let Err(error)=push(&signals.remote,candidate,mid) {self.closer.close();return Err(error);}
        signals.remote_revision.fetch_add(1,Ordering::AcqRel);self.events.changes.changed();Ok(())
    }
}
pub(super) fn candidate(candidate:String,mid:String)->RTCIceCandidateInit {
    RTCIceCandidateInit {candidate,sdp_mid:Some(mid),sdp_mline_index:Some(0),username_fragment:None,url:None}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trickle_queues_charge_consumed_candidates_and_reject_unbounded_input() {
        let signals=Signals::default();
        for _ in 0..MAX_CANDIDATES {signals.local("candidate:fixture".into(),"0".into()).unwrap();signals.local.lock().unwrap().pending.pop_front();}
        assert_eq!(signals.local("candidate:fixture".into(),"0".into()),Err(WebRtcError::Configuration));
        for (candidate,mid) in [("".into(),"0".into()),("x".repeat(MAX_CANDIDATE_BYTES+1),"0".into()),("candidate:fixture\n".into(),"0".into()),("candidate:fixture".into(),"x".repeat(257))] {
            assert_eq!(push(&signals.remote,candidate,mid),Err(WebRtcError::Configuration));
        }
        assert!(signals.remote.lock().unwrap().pending.is_empty());
    }
}
