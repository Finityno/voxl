//! Candidate application must wake the SDK driver without relying on unrelated
//! STUN, TURN, RTCP or media timers. Real UDP remains active in this fixture.
use super::*;
use std::{future::Future,io,net::{SocketAddr,UdpSocket},pin::Pin};
use webrtc::runtime::{Runtime,TokioRuntime,AsyncUdpSocket,AsyncTcpListener,AsyncTcpStream,AsyncInterval,JoinHandle};

#[derive(Debug,Default)]
struct Park {generation:AtomicU64,parked:AtomicBool,wake:Notify}
impl Park {
    async fn after(&self,generation:u64) {
        loop {
            let next=self.wake.notified();tokio::pin!(next);next.as_mut().enable();
            if self.parked.load(Ordering::Acquire) && self.generation.load(Ordering::Acquire)>generation {return;}
            next.await;
        }
    }
}
#[derive(Debug)]
struct NoTimers {inner:Arc<dyn Runtime>,park:Arc<Park>}
impl Runtime for NoTimers {
    fn preferred_turn_local_address(&self,server:SocketAddr,available:&[SocketAddr])->Option<SocketAddr> {self.inner.preferred_turn_local_address(server,available)}
    fn turn_gathering_delay(&self)->Duration {self.inner.turn_gathering_delay()}
    fn spawn(&self,future:Pin<Box<dyn Future<Output=()>+Send>>)->Box<dyn JoinHandle> {self.inner.spawn(future)}
    fn spawn_reactor(&self,size:usize,future:Pin<Box<dyn Future<Output=()>+Send>>)->Box<dyn JoinHandle> {self.inner.spawn_reactor(size,future)}
    fn wrap_udp_socket(&self,socket:UdpSocket)->io::Result<Arc<dyn AsyncUdpSocket>> {self.inner.wrap_udp_socket(socket)}
    fn wrap_tcp_listener(&self,listener:std::net::TcpListener)->io::Result<Arc<dyn AsyncTcpListener>> {self.inner.wrap_tcp_listener(listener)}
    fn connect_tcp<'a>(&'a self,remote:SocketAddr)->Pin<Box<dyn Future<Output=io::Result<Arc<dyn AsyncTcpStream>>>+Send+'a>> {self.inner.connect_tcp(remote)}
    fn resolve_host<'a>(&'a self,host:&'a str)->Pin<Box<dyn Future<Output=io::Result<Vec<SocketAddr>>>+Send+'a>> {self.inner.resolve_host(host)}
    fn now(&self)->Instant {self.inner.now()}
    fn sleep(&self,_duration:Duration)->Pin<Box<dyn Future<Output=()>+Send+'static>> {
        let park=Arc::clone(&self.park);
        Box::pin(async move {
            struct Guard(Arc<Park>);
            impl Drop for Guard {fn drop(&mut self) {self.0.parked.store(false,Ordering::Release);self.0.wake.notify_waiters();}}
            let _guard=Guard(Arc::clone(&park));
            park.parked.store(true,Ordering::Release);park.generation.fetch_add(1,Ordering::AcqRel);park.wake.notify_waiters();
            std::future::pending::<()>().await;
        })
    }
    fn interval(&self,period:Duration)->Box<dyn AsyncInterval> {self.inner.interval(period)}
    fn block_on(&self,future:Pin<Box<dyn Future<Output=()>+'_>>) {self.inner.block_on(future);}
    fn yield_now(&self)->Pin<Box<dyn Future<Output=()>+Send+'static>> {self.inner.yield_now()}
    fn name(&self)->&'static str {"test-no-driver-timers"}
}

#[test]
fn late_trickle_candidates_wake_parked_drivers_without_timer_assistance() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5),async {
            let sender_park=Arc::new(Park::default());let park=Arc::clone(&sender_park);
            let config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
            let mut video=WebRtcVideo::open_with_runtime(config,None,true,Duration::ZERO,move |inner|Arc::new(NoTimers {inner,park})).await.unwrap();
            let signals=video.signaling().unwrap();
            let mut media=MediaEngine::default();media.register_default_codecs().unwrap();let (sender,mut packets)=mpsc::channel(4);
            let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());let receiver_park=Arc::new(Park::default());
            let receiver=PeerConnectionBuilder::new().with_runtime(Arc::new(NoTimers {inner:Arc::new(TokioRuntime),park:Arc::clone(&receiver_park)}))
                .with_media_engine(media).with_handler(Arc::new(TrickleReceiver {receiver:ReceiverEvents {packets:sender},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake)}))
                .with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap();
            let without_candidates=|sdp:&str|sdp.lines().filter(|line|!line.starts_with("a=candidate:") && *line!="a=end-of-candidates").collect::<Vec<_>>().join("\r\n")+"\r\n";
            receiver.set_remote_description(RTCSessionDescription::offer(without_candidates(video.offer())).unwrap()).await.unwrap();
            receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
            while !gathered.load(Ordering::Acquire) {wake.notified().await;}
            sender_park.after(0).await;receiver_park.after(0).await;
            let offer=video.peer.local_description().await.unwrap().sdp;
            let answer=receiver.local_description().await.unwrap().sdp;
            let sender_before_answer=sender_park.generation.load(Ordering::Acquire);
            video.answer(without_candidates(&answer)).await.unwrap();
            // Drain the answer's own driver wake before adding any candidates.
            // Both peers are now parked, and their timer futures never finish.
            sender_park.after(sender_before_answer).await;receiver_park.after(0).await;
            let candidates=|sdp:&str| {
                let mid=sdp.lines().find_map(|line|line.strip_prefix("a=mid:")).unwrap().to_owned();
                sdp.lines().filter_map(|line|line.strip_prefix("a=candidate:")).map(|line|(format!("candidate:{line}"),mid.clone())).collect::<Vec<_>>()
            };
            let remote=candidates(&answer);let local=candidates(&offer);assert!(!local.is_empty() && !remote.is_empty());
            for (candidate,mid) in remote {signals.add_candidate(candidate,mid).unwrap();}
            for (candidate,mid) in local {receiver.add_ice_candidate(super::super::signaling::candidate(candidate,mid)).await.unwrap();}
            let outcome=tokio::time::timeout(Duration::from_millis(250),async {
                assert_eq!(video.next_update().await,VideoUpdate::Connected);
                video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,9,8,7,6],keyframe:true,presentation_time_micros:1_000_000}).await.unwrap();
                assert_eq!(&packets.recv().await.unwrap().payload[..],&[0x65,9,8,7,6]);
                assert_eq!(video.route_statistics().await.route,MediaRoute::Direct);
            }).await;
            video.close();receiver.close().await.unwrap();
            outcome.expect("late candidates must wake idle drivers without a timer expiring");
        }).await.expect("bounded idle candidate fixture");
    });
}
