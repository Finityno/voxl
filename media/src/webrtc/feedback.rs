//! Observe PLI/FIR at the application end of the interceptor chain. Keep a
//! single pending keyframe request, not an unbounded queue of feedback events.
use std::collections::VecDeque;
use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
use std::time::Instant;
use rtc::interceptor::{Interceptor,Packet,StreamInfo,TaggedPacket};
use rtc::sansio::Protocol;
use rtc::shared::error::Error;
use rtc::rtcp::payload_feedbacks::{picture_loss_indication::PictureLossIndication, full_intra_request::FullIntraRequest};

pub(super) struct KeyframeFeedback {
    keyframe: Arc<AtomicBool>,
    read: VecDeque<TaggedPacket>,
    write: VecDeque<TaggedPacket>,
}
impl KeyframeFeedback {
    pub fn new(keyframe: Arc<AtomicBool>) -> Self { Self {keyframe,read: VecDeque::new(),write: VecDeque::new()} }
}
impl Protocol<TaggedPacket,TaggedPacket,()> for KeyframeFeedback {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = Error;
    type Time = Instant;
    fn handle_read(&mut self,message: TaggedPacket) -> Result<(),Error> {
        if let Packet::Rtcp(packets) = &message.message.packet
            && packets.iter().any(|packet| packet.as_any().is::<PictureLossIndication>() || packet.as_any().is::<FullIntraRequest>()) {
            self.keyframe.store(true,Ordering::Release);
        }
        if self.read.len() >= 64 { return Err(Error::Other("desktop feedback queue is full".into())); }
        self.read.push_back(message);
        Ok(())
    }
    fn poll_read(&mut self) -> Option<TaggedPacket> { self.read.pop_front() }
    fn handle_write(&mut self,message: TaggedPacket) -> Result<(),Error> {
        if self.write.len() >= 64 { return Err(Error::Other("desktop media queue is full".into())); }
        self.write.push_back(message);
        Ok(())
    }
    fn poll_write(&mut self) -> Option<TaggedPacket> { self.write.pop_front() }
}
impl Interceptor for KeyframeFeedback {
    fn bind_local_stream(&mut self,_info: &StreamInfo) {}
    fn unbind_local_stream(&mut self,_info: &StreamInfo) {}
    fn bind_remote_stream(&mut self,_info: &StreamInfo) {}
    fn unbind_remote_stream(&mut self,_info: &StreamInfo) {}
}
