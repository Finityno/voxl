//! Owned encrypted-UDP bottleneck. SDP changes only the loopback candidate
//! ports; DTLS fingerprints/ICE credentials are untouched. No decoder, physical
//! network, TURN, native codec, quality or end-to-end performance admission.
use super::*;
use std::{collections::{BTreeMap,VecDeque},net::SocketAddr};
use tokio::{net::UdpSocket,sync::oneshot,task::{JoinHandle,JoinSet}};

const QUEUE_LIMIT:usize=256;
#[derive(Clone,Copy)]
struct Shape {bps:u32,delay:Duration,jitter:Duration,drop_rtp:Option<u64>}
impl Shape {
    fn new(bps:u32,delay_ms:u64,jitter_ms:u64,drop_rtp:Option<u64>)->Self {
        assert!((250_000..=20_000_000).contains(&bps) && delay_ms<=100 && jitter_ms<=20);
        Self {bps,delay:Duration::from_millis(delay_ms),jitter:Duration::from_millis(jitter_ms),drop_rtp}
    }
}
#[derive(Clone,Default,Debug)]
struct LinkStats {rtp:u64,delivered:u64,dropped:u64,overflow:u64,duplicates:u64,bytes:u64,peak_queue:usize,max_queue_micros:u64}
struct Queued {at:Instant,finished:Instant,delay:Duration,received:Instant,bytes:Vec<u8>}
struct Direction {queue:VecDeque<Queued>,available:Instant,last_due:Instant,seen:VecDeque<u16>,stats:LinkStats}
impl Direction {
    fn new()->Self {let now=Instant::now();Self {queue:VecDeque::new(),available:now,last_due:now,seen:VecDeque::new(),stats:LinkStats::default()}}
    fn enqueue(&mut self,bytes:&[u8],now:Instant,shape:Shape) {
        let rtp=bytes.len()>=12 && bytes[0]&0xc0==0x80 && !(192..=223).contains(&bytes[1]);
        if rtp {
            self.stats.rtp+=1;let sequence=u16::from_be_bytes([bytes[2],bytes[3]]);
            if self.seen.contains(&sequence) {self.stats.duplicates+=1;} else {self.seen.push_back(sequence);if self.seen.len()>1024 {self.seen.pop_front();}}
            if shape.drop_rtp==Some(self.stats.rtp) {self.stats.dropped+=1;return;}
        }
        if self.queue.len()>=QUEUE_LIMIT {self.stats.overflow+=1;return;}
        // Serialized UDP bytes, including SRTP/RTCP and handshake/control.
        // Delay/jitter do not replenish serialization credit or reorder FIFO.
        self.available=self.available.max(now)+Duration::from_secs_f64(bytes.len() as f64*8.0/f64::from(shape.bps));
        let jitter=if self.stats.rtp.is_multiple_of(4) {shape.jitter} else {Duration::ZERO};
        let at=(self.available+shape.delay+jitter).max(self.last_due);self.last_due=at;
        self.queue.push_back(Queued {at,finished:self.available,delay:shape.delay+jitter,received:now,bytes:bytes.to_vec()});self.stats.peak_queue=self.stats.peak_queue.max(self.queue.len());
    }
    fn reconfigure(&mut self,old_bps:u32,new_bps:u32,now:Instant) {
        // Service already performed is spent. Remaining serialization follows
        // the new capacity; bytes already propagating keep their arrival time.
        let mut finish=now;let mut due=now;
        for packet in &mut self.queue {
            if packet.finished>now {
                let bits=(packet.finished.duration_since(now).as_secs_f64()*f64::from(old_bps)).min(packet.bytes.len() as f64*8.0);
                finish+=Duration::from_secs_f64(bits/f64::from(new_bps));packet.finished=finish;packet.at=(finish+packet.delay).max(due);
            }
            due=packet.at;
        }
        self.available=finish;self.last_due=due;
    }
    fn next(&self)->Option<Instant> {self.queue.front().map(|packet|packet.at)}
    async fn release(&mut self,socket:&UdpSocket,peer:SocketAddr,now:Instant) {
        while self.next().is_some_and(|at|at<=now) {
            let packet=self.queue.pop_front().unwrap();let sent=socket.send_to(&packet.bytes,peer).await.unwrap();assert_eq!(sent,packet.bytes.len());
            self.stats.delivered+=1;self.stats.bytes+=sent as u64;self.stats.max_queue_micros=self.stats.max_queue_micros.max(now.duration_since(packet.received).as_micros() as u64);
        }
    }
}
enum Command {Condition(Shape,oneshot::Sender<()>),Snapshot(oneshot::Sender<LinkStats>)}
enum LinkTask {Running(JoinHandle<()>),Ended}
struct Link {sender_address:SocketAddr,receiver_address:SocketAddr,commands:mpsc::Sender<Command>,task:LinkTask}
impl Drop for Link {fn drop(&mut self) {if let LinkTask::Running(task)=&self.task {task.abort();}}}
impl Link {
    async fn start(sender:SocketAddr,receiver:SocketAddr,a:UdpSocket,b:UdpSocket)->Self {
        assert!(sender.ip().is_loopback() && receiver.ip().is_loopback());
        let sender_address=a.local_addr().unwrap();let receiver_address=b.local_addr().unwrap();let (commands,mut changes)=mpsc::channel(4);
        let task=tokio::spawn(async move {
            let mut forward=Direction::new();let mut reverse=Direction::new();let mut shape:Option<Shape>=None;let mut from_a=vec![0u8;65_536];let mut from_b=vec![0u8;65_536];
            loop {
                let next=match (forward.next(),reverse.next()) {(Some(a),Some(b))=>a.min(b),(Some(at),None)|(None,Some(at))=>at,(None,None)=>Instant::now()+Duration::from_secs(30)};
                tokio::select! {
                    command=changes.recv()=>match command {
                        Some(Command::Condition(value,done))=>{if let Some(old)=shape {let now=Instant::now();forward.reconfigure(old.bps,value.bps,now);reverse.reconfigure(old.bps,value.bps,now);}shape=Some(value);let _=done.send(());},
                        Some(Command::Snapshot(reply))=>{let _=reply.send(forward.stats.clone());},None=>break,
                    },
                    packet=a.recv_from(&mut from_a)=>{
                        let (length,source)=packet.unwrap();assert_eq!(source,receiver);
                        if let Some(shape)=shape {reverse.enqueue(&from_a[..length],Instant::now(),Shape {drop_rtp:None,..shape});} else {b.send_to(&from_a[..length],sender).await.unwrap();}
                    },
                    packet=b.recv_from(&mut from_b)=>{
                        let (length,source)=packet.unwrap();assert_eq!(source,sender);
                        if let Some(shape)=shape {forward.enqueue(&from_b[..length],Instant::now(),shape);} else {a.send_to(&from_b[..length],receiver).await.unwrap();}
                    },
                    _=tokio::time::sleep_until(next.into())=>{
                        let now=Instant::now();forward.release(&a,receiver,now).await;reverse.release(&b,sender,now).await;
                    },
                }
            }
        });
        Self {sender_address,receiver_address,commands,task:LinkTask::Running(task)}
    }
    async fn condition(&self,shape:Shape) {let (reply,done)=oneshot::channel();self.commands.send(Command::Condition(shape,reply)).await.unwrap();done.await.unwrap();}
    async fn snapshot(&self)->LinkStats {let (reply,done)=oneshot::channel();self.commands.send(Command::Snapshot(reply)).await.unwrap();done.await.unwrap()}
    async fn close(&mut self) {if let LinkTask::Running(task)=std::mem::replace(&mut self.task,LinkTask::Ended) {task.abort();assert!(task.await.unwrap_err().is_cancelled());}}
}
struct Receiver {packets:mpsc::Sender<rtc::rtp::Packet>,tasks:Mutex<JoinSet<()>>,gathered:AtomicBool,gather_wake:Notify}
impl Drop for Receiver {fn drop(&mut self) {self.tasks.get_mut().unwrap().abort_all();}}
impl Receiver {
    async fn close(&self) {let mut tasks=std::mem::take(&mut *self.tasks.lock().unwrap());tasks.abort_all();while let Some(result)=tasks.join_next().await {assert!(result.is_ok() || result.unwrap_err().is_cancelled());}}
}
#[async_trait]
impl PeerConnectionEventHandler for Receiver {
    async fn on_ice_gathering_state_change(&self,state:RTCIceGatheringState) {if state==RTCIceGatheringState::Complete {self.gathered.store(true,Ordering::Release);self.gather_wake.notify_one();}}
    async fn on_track(&self,track:Arc<dyn TrackRemote>) {
        let packets=self.packets.clone();self.tasks.lock().unwrap().spawn(async move {
            while let Some(event)=track.poll().await {if let TrackRemoteEvent::OnRtpPacket(packet)=event && packets.try_send(packet).is_err() {break;}}
        });
    }
}
fn candidate(sdp:&str,proxy:SocketAddr)->(String,SocketAddr) {
    let mut actual=None;let mut lines=Vec::new();
    for line in sdp.lines() {
        if line.starts_with("a=candidate:") {
            let mut fields:Vec<_>=line.split_whitespace().map(str::to_owned).collect();
            assert!(fields.len()>=8 && fields[2].eq_ignore_ascii_case("udp") && fields[7]=="host");
            let address:SocketAddr=format!("{}:{}",fields[4],fields[5]).parse().unwrap();assert!(address.ip().is_loopback());if let Some(previous)=actual {assert_eq!(address,previous,"fixture candidates must share the same owned socket");}actual=Some(address);
            fields[4]=proxy.ip().to_string();fields[5]=proxy.port().to_string();lines.push(fields.join(" "));
        } else {lines.push(line.to_owned());}
    }
    (lines.join("\r\n")+"\r\n",actual.unwrap())
}
async fn pair_with_delay(delay:DelayControl)->(WebRtcVideo,Arc<dyn PeerConnection>,Arc<Receiver>,mpsc::Receiver<rtc::rtp::Packet>,Link) {
    pair_with_control(SendControl::new(250,1500,1500).unwrap().with_pacing_gain(PacingGain::Responsive).with_delay_control(delay)).await
}
async fn pair_with_control(control:SendControl)->(WebRtcVideo,Arc<dyn PeerConnection>,Arc<Receiver>,mpsc::Receiver<rtc::rtp::Packet>,Link) {
    let config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
    let mut video=WebRtcVideo::open_controlled(config,control).await.unwrap();
    let a=UdpSocket::bind("127.0.0.1:0").await.unwrap();let b=UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (offer,sender_address)=candidate(video.offer(),a.local_addr().unwrap());
    let mut media=MediaEngine::default();media.register_default_codecs().unwrap();let registry=register_default_interceptors(Registry::new(),&mut media).unwrap();
    let (packets,received)=mpsc::channel(256);let events=Arc::new(Receiver {packets,tasks:Mutex::new(JoinSet::new()),gathered:AtomicBool::new(false),gather_wake:Notify::new()});
    let receiver:Arc<dyn PeerConnection>=Arc::new(PeerConnectionBuilder::new().with_media_engine(media).with_interceptor_registry(registry).with_handler(events.clone()).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap());
    receiver.set_remote_description(RTCSessionDescription::offer(offer).unwrap()).await.unwrap();receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
    while !events.gathered.load(Ordering::Acquire) {events.gather_wake.notified().await;}
    let (answer,receiver_address)=candidate(&receiver.local_description().await.unwrap().sdp,b.local_addr().unwrap());
    let link=Link::start(sender_address,receiver_address,a,b).await;assert_ne!(link.sender_address,link.receiver_address);
    video.answer(answer).await.unwrap();assert_eq!(video.next_update().await,VideoUpdate::Connected);
    assert_eq!(video.route_statistics().await.route,MediaRoute::Direct);
    (video,receiver,events,received,link)
}
fn frame(bytes:usize,keyframe:bool,pts:i64)->EncodedFrame {let mut annex_b=vec![0x77;bytes];annex_b[..5].copy_from_slice(&[0,0,0,1,if keyframe {0x65} else {0x41}]);EncodedFrame {codec:VideoCodec::H264,annex_b,keyframe,presentation_time_micros:pts}}
async fn exact_frame(packets:&mut mpsc::Receiver<rtc::rtp::Packet>,expected:&[u8],timestamp:u32) {
    let mut pieces=BTreeMap::new();let mut first=None;let mut marker=false;let mut bytes=0;
    while !marker || bytes<expected.len() {
        let packet=packets.recv().await.unwrap();assert_eq!(packet.header.timestamp,timestamp);
        let mut payload=Vec::new();
        if packet.payload[0]&31==28 {if packet.payload[1]&0x80!=0 {payload.push((packet.payload[0]&0xe0)|(packet.payload[1]&31));}payload.extend_from_slice(&packet.payload[2..]);} else {payload.extend_from_slice(&packet.payload);}
        let origin=*first.get_or_insert(packet.header.sequence_number);
        if let std::collections::btree_map::Entry::Vacant(entry)=pieces.entry(packet.header.sequence_number.wrapping_sub(origin)) {bytes+=payload.len();entry.insert(payload);}marker|=packet.header.marker;
        assert!(pieces.len()<=256 && bytes<=expected.len());
    }
    assert_eq!(pieces.into_values().flatten().collect::<Vec<_>>(),expected);
}
#[test]
fn adaptive_network_fixture_serializes_caps_queue_and_never_resets_credit_on_capacity_change() {
    let mut direction=Direction::new();let now=Instant::now();let packet=vec![0;1000];let slow=Shape::new(1_000_000,20,10,None);
    direction.enqueue(&packet,now,slow);direction.enqueue(&packet,now,slow);
    assert_eq!(direction.queue[1].at-direction.queue[0].at,Duration::from_millis(8));direction.reconfigure(1_000_000,500_000,now+Duration::from_millis(4));
    assert_eq!(direction.queue[0].finished,now+Duration::from_millis(12));assert_eq!(direction.queue[1].finished,now+Duration::from_millis(28));
    direction.enqueue(&packet,now+Duration::from_millis(4),Shape::new(500_000,20,0,None));assert_eq!(direction.available,now+Duration::from_millis(44));
    for _ in 0..QUEUE_LIMIT {direction.enqueue(&packet,now,slow);}
    assert_eq!(direction.queue.len(),QUEUE_LIMIT);assert_eq!(direction.stats.peak_queue,QUEUE_LIMIT);assert_eq!(direction.stats.overflow,3);
}
#[test]
fn adaptive_network_headroom_exposes_receiver_queueing_behind_a_slower_encrypted_udp_path() {bottleneck(DelayControl::Standard);}
#[test]
fn adaptive_network_fast_delay_preserves_a_large_keyframe_behind_the_same_bottleneck() {bottleneck(DelayControl::Trendline);}
#[test]
fn adaptive_network_quiet_resume_exposes_the_old_capacity_budget_after_a_silent_link_change() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {tokio::time::timeout(Duration::from_secs(15),async {
        let (mut video,receiver,events,mut packets,mut link)=pair_with_delay(DelayControl::Trendline).await;
        link.condition(Shape::new(3_000_000,10,0,None)).await;
        let first=frame(24_005,true,1_000_000);let expected=first.annex_b[4..].to_vec();let stamp=video.send_with_stamp(first).await.unwrap();exact_frame(&mut packets,&expected,stamp.rtp_timestamp).await;
        let feedback=Instant::now()+Duration::from_secs(2);
        while video.bitrate_estimate().is_none() && Instant::now()<feedback {tokio::time::sleep(Duration::from_millis(10)).await;}
        let before=video.bitrate_estimate().expect("actual TWCC before quiet");let quiet=link.snapshot().await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let after=link.snapshot().await;assert_eq!(after.rtp,quiet.rtp,"no synthetic video or RTP is needed while quiet");
        link.condition(Shape::new(250_000,10,0,None)).await;
        let held=video.close_handle().frame_budget().unwrap();assert!(held.estimate_age.is_some_and(|age|age>=Duration::from_secs(2)));
        let unit=frame(64_005,true,5_000_000);assert!(unit.annex_b.len()<=held.payload_bytes as usize);
        let expected=unit.annex_b[4..].to_vec();let started=Instant::now();let stamp=video.send_with_stamp(unit).await.unwrap();let sent=started.elapsed();
        exact_frame(&mut packets,&expected,stamp.rtp_timestamp).await;let arrived=started.elapsed();let stats=link.snapshot().await;
        eprintln!("quiet_resume_baseline quiet_rtp_packets={} held_media_kbps={} estimate_age_us={} frame_budget_bytes={} resume_bytes=64005 send_us={} receiver_us={} queue_max_us={} overflow={}",after.rtp-quiet.rtp,before.target_kbps,held.estimate_age.unwrap().as_micros(),held.payload_bytes,sent.as_micros(),arrived.as_micros(),stats.max_queue_micros,stats.overflow);
        assert!(sent<Duration::from_millis(500));assert!(arrived>Duration::from_millis(1500));assert!(stats.max_queue_micros>1_000_000);assert_eq!(stats.overflow,0);
        video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();events.close().await;link.close().await;
    }).await.expect("bounded quiet/capacity-change baseline");});
}
#[test]
fn adaptive_network_quiet_probe_bounds_resume_after_capacity_falls() {quiet_probe(250_000);}
#[test]
fn adaptive_network_quiet_probe_preserves_the_large_frame_on_an_unchanged_fast_path() {quiet_probe(3_000_000);}
fn quiet_probe(capacity:u32) {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {tokio::time::timeout(Duration::from_secs(15),async {
        let limits=SendControl::new(250,1500,1500).unwrap().with_pacing_gain(PacingGain::Responsive).with_delay_control(DelayControl::Trendline).with_quiet_resume_probes().with_flow_observation();
        let (mut video,receiver,events,mut packets,mut link)=pair_with_control(limits).await;link.condition(Shape::new(3_000_000,10,0,None)).await;
        assert!(matches!(video.probe_quiet_resume().await,Err(WebRtcError::Configuration)));assert!(!video.ended());
        let first=frame(24_005,true,1_000_000);let expected=first.annex_b[4..].to_vec();let stamp=video.send_with_stamp(first).await.unwrap();exact_frame(&mut packets,&expected,stamp.rtp_timestamp).await;
        let deadline=Instant::now()+Duration::from_secs(2);while video.bitrate_estimate().is_none() && Instant::now()<deadline {tokio::time::sleep(Duration::from_millis(10)).await;}
        assert!(video.bitrate_estimate().is_some());let quiet=link.snapshot().await;tokio::time::sleep(Duration::from_secs(3)).await;assert_eq!(link.snapshot().await.rtp,quiet.rtp);
        link.condition(Shape::new(capacity,10,0,None)).await;let began=Instant::now();let observed=video.probe_quiet_resume().await.unwrap();let probe_time=began.elapsed();
        for _ in 0..observed.packets {let packet=packets.recv().await.unwrap();assert!(packet.header.padding && !packet.header.marker && packet.payload.is_empty());assert_eq!(packet.header.timestamp,stamp.rtp_timestamp);}
        assert_eq!((observed.packets,observed.arrived),(32,32));assert_eq!(video.close_handle().flow_observation().unwrap().completed_frames,1,"padding cannot manufacture a completed video picture");
        let budget=video.close_handle().frame_budget().unwrap();let bytes=64_005usize.min((u64::from(budget.payload_bytes)*4/5) as usize);let unit=frame(bytes,true,5_000_000);let expected=unit.annex_b[4..].to_vec();let stamp=video.send_with_stamp(unit).await.unwrap();
        exact_frame(&mut packets,&expected,stamp.rtp_timestamp).await;let total=began.elapsed();let stats=link.snapshot().await;
        eprintln!("quiet_resume_probe capacity_bps={capacity} padding_packets={} padding_rtp_bytes={} receiver_span_us={} measured_rtp_bps={} target_kbps={} frame_budget_bytes={} resume_bytes={bytes} probe_us={} total_resume_us={} queue_max_us={} overflow={}",observed.packets,observed.received_rtp_bytes,observed.arrival_span.as_micros(),observed.received_bps,observed.target_kbps,budget.payload_bytes,probe_time.as_micros(),total.as_micros(),stats.max_queue_micros,stats.overflow);
        assert!(probe_time<Duration::from_millis(500) && total<Duration::from_millis(750));assert_eq!(stats.overflow,0);
        if capacity==3_000_000 {assert_eq!(bytes,64_005);assert!(observed.target_kbps>=1000);}else {assert_eq!(observed.target_kbps,250);assert!(bytes<10_000);}
        assert_eq!(video.close_handle().flow_observation().unwrap().completed_frames,2);assert!(!video.ended());
        video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();events.close().await;link.close().await;
    }).await.expect("bounded actual padding/TWCC quiet resume");});
}
fn bottleneck(delay:DelayControl) {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {tokio::time::timeout(Duration::from_secs(12),async {
        let (mut video,receiver,events,mut packets,mut link)=pair_with_delay(delay).await;link.condition(Shape::new(1_500_000,30,10,None)).await;
        let unit=frame(103_281,true,1_000_000);let expected=unit.annex_b[4..].to_vec();let started=Instant::now();let stamp=video.send_with_stamp(unit).await.unwrap();let send=started.elapsed();
        exact_frame(&mut packets,&expected,stamp.rtp_timestamp).await;let arrival=started.elapsed();let stats=link.snapshot().await;
        eprintln!("headroom_bottleneck send_us={} receiver_us={} queued_us={} peak_queue={} udp_bytes={} drops={} duplicates={}",send.as_micros(),arrival.as_micros(),stats.max_queue_micros,stats.peak_queue,stats.bytes,stats.dropped,stats.duplicates);
        assert!(send<Duration::from_millis(500));assert!(arrival>=Duration::from_millis(500) && arrival<Duration::from_secs(3));assert!(stats.peak_queue>=10 && stats.max_queue_micros>=100_000);assert_eq!(stats.overflow,0);assert_eq!(stats.dropped,0);
        video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();events.close().await;link.close().await;
    }).await.expect("bounded encrypted UDP bottleneck");});
}
#[test]
fn adaptive_network_single_packet_loss_is_repaired_without_partial_access_unit_or_unbounded_queue() {loss(DelayControl::Standard);}
#[test]
fn adaptive_network_fast_delay_retransmits_one_lost_packet_without_losing_the_access_unit() {loss(DelayControl::Trendline);}
fn loss(delay:DelayControl) {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {tokio::time::timeout(Duration::from_secs(12),async {
        let (mut video,receiver,events,mut packets,mut link)=pair_with_delay(delay).await;link.condition(Shape::new(2_000_000,15,5,Some(4))).await;
        let unit=frame(24_005,true,1_000_000);let expected=unit.annex_b[4..].to_vec();let stamp=video.send_with_stamp(unit).await.unwrap();
        exact_frame(&mut packets,&expected,stamp.rtp_timestamp).await;let stats=link.snapshot().await;
        eprintln!("headroom_loss rtp={} dropped={} duplicates={} peak_queue={} queued_us={}",stats.rtp,stats.dropped,stats.duplicates,stats.peak_queue,stats.max_queue_micros);
        assert_eq!(stats.dropped,1);assert!(stats.duplicates>=1);assert_eq!(stats.overflow,0);assert!(stats.peak_queue<=QUEUE_LIMIT);assert!(!video.ended());
        video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();events.close().await;link.close().await;
    }).await.expect("bounded loss/retransmission");});
}
#[test]
fn adaptive_network_real_twcc_reduces_media_target_when_capacity_falls_and_keeps_recovery_bounded() {capacity(DelayControl::Standard,false);}
#[test]
fn adaptive_network_fast_delay_keeps_the_transient_response_and_queue_within_the_new_gate() {capacity(DelayControl::Trendline,true);}
fn capacity(delay:DelayControl,fast:bool) {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {tokio::time::timeout(Duration::from_secs(25),async {
        let (mut video,receiver,events,mut packets,mut link)=pair_with_delay(delay).await;let consumed=Arc::new(AtomicU64::new(0));let seen=Arc::clone(&consumed);
        let reader=tokio::spawn(async move {while packets.recv().await.is_some() {seen.fetch_add(1,Ordering::Relaxed);}});
        let mut sequence=0;let mut low_min=1_500_000;let mut recovered_max=0;let mut recovered_final=0;let mut estimates=0;let mut encoder_target=1275;let experiment=Instant::now();let mut traces=0;let mut last_trace=None;let mut constrained=None;let mut first_reduction=None;
        for (stage,bps,seconds) in [(0,3_000_000,2),(1,500_000,5),(2,3_000_000,3)] {
            link.condition(Shape::new(bps,20,5,None)).await;if stage==1 {constrained=Some(Instant::now());}let until=Instant::now()+Duration::from_secs(seconds);let mut interval=tokio::time::interval(Duration::from_millis(33));interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            while Instant::now()<until {
                interval.tick().await;
                if let Some(estimate)=video.bitrate_estimate() {assert!((250..=1500).contains(&estimate.target_kbps));estimates+=1;if stage==1 {low_min=low_min.min(estimate.target_kbps*1000);if estimate.target_kbps<1000 && first_reduction.is_none() {first_reduction=constrained.map(|at|at.elapsed());}}else if stage==2 {recovered_final=estimate.target_kbps*1000;recovered_max=recovered_max.max(recovered_final);}}
                // The native worker holds its last verified target on stale or
                // unknown feedback; it never jumps back to the initial prior.
                if let Some(target)=video.close_handle().bitrate_target_kbps() {encoder_target=target;}
                if let Some(estimate)=video.bitrate_estimate() && traces<32 && last_trace!=Some((stage,estimate.target_kbps)) {
                    eprintln!("headroom_target stage={stage} elapsed_us={} media_kbps={} held_producer_kbps={encoder_target} estimate_age_us={}",experiment.elapsed().as_micros(),estimate.target_kbps,estimate.age.as_micros());last_trace=Some((stage,estimate.target_kbps));traces+=1;
                }
                let bytes=(encoder_target as usize*1000/8/30).max(900);sequence+=1;
                video.send(frame(bytes,sequence==1 || sequence%60==0,sequence*33_333)).await.unwrap();
            }
            let s=link.snapshot().await;eprintln!("headroom_capacity stage={stage} capacity_bps={bps} sent_frames={sequence} media_min_bps={low_min} recovery_max_bps={recovered_max} recovery_final_bps={recovered_final} rtp={} delivered={} overflow={} peak_queue={} queue_max_us={} received_packets={}",s.rtp,s.delivered,s.overflow,s.peak_queue,s.max_queue_micros,consumed.load(Ordering::Relaxed));
            assert!(s.peak_queue<=QUEUE_LIMIT);
            if stage==1 {
                eprintln!("headroom_transient first_reduction_us={} max_residence_us={}",first_reduction.map_or(u128::MAX,|elapsed|elapsed.as_micros()),s.max_queue_micros);
                if fast {assert!(first_reduction.is_some_and(|elapsed|elapsed<=Duration::from_millis(500)),"first media reduction must occur within500ms of the capacity drop");
                assert!(s.max_queue_micros<=750_000,"transient queue residence must stay below750ms");}
            }
        }
        assert!(estimates>0 && low_min<1_000_000,"real feedback must lower the media target below the offered load");assert!(recovered_final>low_min*11/10 && !video.ended(),"capacity recovery must raise the final media target by more than ten percent");
        video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();events.close().await;link.close().await;reader.abort();assert!(reader.await.unwrap_err().is_cancelled());
    }).await.expect("bounded capacity drop/recovery");});
}
