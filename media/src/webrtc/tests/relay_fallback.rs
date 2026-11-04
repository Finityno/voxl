//! Real UDP/DTLS/SRTP after late ICE configuration. No capture, input or decoder.
use super::*;
use std::{future::Future,io,net::{IpAddr,SocketAddr},pin::Pin};
use webrtc::runtime::{Runtime,AsyncUdpSocket,AsyncTcpListener,AsyncTcpStream,AsyncInterval,JoinHandle};

#[derive(Clone,Copy)]
enum InitialDiscovery { Empty, SilentUdp, PendingDns }
#[derive(Debug,Default)]
struct DnsProbe {entered:Notify,cancelled:AtomicBool}
#[derive(Debug)]
enum DnsAnswer {Pending,Ready(SocketAddr)}
#[derive(Debug)]
struct DnsGate {answer:std::sync::Mutex<DnsAnswer>,changed:Notify,parks:AtomicU64,started:AtomicU64,parked:Notify}
impl Default for DnsGate {
    fn default()->Self {Self {answer:std::sync::Mutex::new(DnsAnswer::Pending),changed:Notify::new(),parks:AtomicU64::new(0),started:AtomicU64::new(0),parked:Notify::new()}}
}
impl DnsGate {
    async fn resolve(&self)->io::Result<Vec<SocketAddr>> {
        loop {
            let changed=self.changed.notified();tokio::pin!(changed);changed.as_mut().enable();
            if let DnsAnswer::Ready(address)=*self.answer.lock().unwrap() {return Ok(vec![address]);}
            changed.await;
        }
    }
    async fn wait_parked(&self) {
        loop {
            let parked=self.parked.notified();tokio::pin!(parked);parked.as_mut().enable();
            if self.parks.load(Ordering::Acquire)>self.started.load(Ordering::Acquire) {return;}
            parked.await;
        }
    }
}
#[derive(Debug)]
enum DnsMode {Pending,Gated(Arc<DnsGate>)}
#[derive(Debug)]
struct PendingDns {inner:Arc<dyn Runtime>,probe:Arc<DnsProbe>,mode:DnsMode}
impl PendingDns {
    fn new(inner:Arc<dyn Runtime>,probe:Arc<DnsProbe>)->Self {Self {inner,probe,mode:DnsMode::Pending}}
}
impl Runtime for PendingDns {
    fn preferred_turn_local_address(&self,server:SocketAddr,available:&[SocketAddr])->Option<SocketAddr> {self.inner.preferred_turn_local_address(server,available)}
    fn turn_gathering_delay(&self)->Duration {self.inner.turn_gathering_delay()}
    fn spawn(&self,future:Pin<Box<dyn Future<Output=()>+Send>>)->Box<dyn JoinHandle> {self.inner.spawn(future)}
    fn spawn_reactor(&self,size:usize,future:Pin<Box<dyn Future<Output=()>+Send>>)->Box<dyn JoinHandle> {self.inner.spawn_reactor(size,future)}
    fn wrap_udp_socket(&self,socket:std::net::UdpSocket)->io::Result<Arc<dyn AsyncUdpSocket>> {self.inner.wrap_udp_socket(socket)}
    fn wrap_tcp_listener(&self,listener:std::net::TcpListener)->io::Result<Arc<dyn AsyncTcpListener>> {self.inner.wrap_tcp_listener(listener)}
    fn connect_tcp<'a>(&'a self,remote:SocketAddr)->Pin<Box<dyn Future<Output=io::Result<Arc<dyn AsyncTcpStream>>>+Send+'a>> {self.inner.connect_tcp(remote)}
    fn resolve_host<'a>(&'a self,host:&'a str)->Pin<Box<dyn Future<Output=io::Result<Vec<SocketAddr>>>+Send+'a>> {
        if !host.starts_with("unanswered.fixture.invalid:") {return self.inner.resolve_host(host);}
        Box::pin(async move {
            struct Cancelled<'a>(&'a AtomicBool);
            impl Drop for Cancelled<'_> {fn drop(&mut self) {self.0.store(true,Ordering::Release);}}
            let _cancelled=Cancelled(&self.probe.cancelled);
            if let DnsMode::Gated(gate)=&self.mode {gate.started.store(gate.parks.load(Ordering::Acquire),Ordering::Release);}
            self.probe.entered.notify_one();
            match &self.mode {DnsMode::Pending=>std::future::pending().await,DnsMode::Gated(gate)=>gate.resolve().await}
        })
    }
    fn now(&self)->Instant {self.inner.now()}
    fn sleep(&self,duration:Duration)->Pin<Box<dyn Future<Output=()>+Send+'static>> {
        if let DnsMode::Gated(gate)=&self.mode {
            let gate=Arc::clone(gate);
            return Box::pin(async move {gate.parks.fetch_add(1,Ordering::AcqRel);gate.parked.notify_one();std::future::pending::<()>().await;});
        }
        self.inner.sleep(duration)
    }
    fn interval(&self,period:Duration)->Box<dyn AsyncInterval> {self.inner.interval(period)}
    fn block_on(&self,future:Pin<Box<dyn Future<Output=()>+'_>>) {self.inner.block_on(future);}
    fn yield_now(&self)->Pin<Box<dyn Future<Output=()>+Send+'static>> {self.inner.yield_now()}
    fn name(&self)->&'static str {"test-pending-stun-dns"}
}
use tokio::{net::UdpSocket,time::timeout};

fn install_diagnostics() {
    struct DiagnosticLog;
    static LOGGER:DiagnosticLog=DiagnosticLog;
    static INIT:std::sync::Once=std::sync::Once::new();
    static COUNT:AtomicU64=AtomicU64::new(0);
    impl log::Log for DiagnosticLog {
        fn enabled(&self,metadata:&log::Metadata<'_>)->bool {metadata.level()<=log::Level::Warn}
        fn log(&self,record:&log::Record<'_>) {
            if !self.enabled(record.metadata()) || COUNT.fetch_add(1,Ordering::Relaxed)>=16 {return;}
            let message=record.args().to_string();
            let class=if message.contains("srtp_context") {"srtp_context"}
                else if message.contains("auth") || message.contains("Auth") {"authentication"}
                else if message.contains("SSRC") || message.contains("ssrc") {"ssrc"}
                else if message.contains("RTP") || message.contains("rtp") {"rtp"}
                else if message.contains("codec") {"codec"} else {"other"};
            // Messages may contain addresses or authentication. Only these
            // fixed categories and the static Rust module name leave memory.
            eprintln!("sdk_diagnostic module={} class={class}",record.target());
        }
        fn flush(&self) {}
    }
    INIT.call_once(|| {if log::set_logger(&LOGGER).is_ok() {log::set_max_level(log::LevelFilter::Warn);}});
}

struct FallbackReceiver {receiver:ReceiverEvents,gathered:Arc<AtomicBool>,wake:Arc<Notify>,connected:Arc<AtomicBool>,track_seen:Arc<AtomicBool>}
#[async_trait]
impl PeerConnectionEventHandler for FallbackReceiver {
    async fn on_track(&self,track:Arc<dyn TrackRemote>) {self.track_seen.store(true,Ordering::Release);self.receiver.on_track(track).await;}
    async fn on_ice_gathering_state_change(&self,state:RTCIceGatheringState) {if state==RTCIceGatheringState::Complete {self.gathered.store(true,Ordering::Release);self.wake.notify_one();}}
    async fn on_connection_state_change(&self,state:RTCPeerConnectionState) {self.connected.store(state==RTCPeerConnectionState::Connected,Ordering::Release);}
}

#[test]
fn late_relay_configuration_restarts_ice_and_fences_old_signaling() {late_configuration(None,true,InitialDiscovery::Empty);}

#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_udp_configuration_carries_media() {late_configuration(Some(RelayTransport::Udp),true,InitialDiscovery::Empty);}
#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_tcp_configuration_carries_media() {late_configuration(Some(RelayTransport::Tcp),true,InitialDiscovery::Empty);}
#[test]
#[ignore = "requires isolated coturn TLS and a namespace blocking direct host UDP"]
fn late_relay_tls_configuration_carries_media() {late_configuration(Some(RelayTransport::Tls),true,InitialDiscovery::Empty);}
#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_udp_initial_configuration_baseline() {late_configuration(Some(RelayTransport::Udp),false,InitialDiscovery::Empty);}

#[test]
fn late_relay_interrupts_pending_stun_udp_and_rejects_late_response() {late_configuration(None,true,InitialDiscovery::SilentUdp);}
#[test]
fn late_relay_interrupts_pending_stun_dns() {late_configuration(None,true,InitialDiscovery::PendingDns);}

#[test]
fn direct_video_connects_while_stun_dns_remains_pending() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {timeout(Duration::from_secs(5),async {
        let format=VideoFormat::H264 {profile_level_id:[66,224,31]};let bindings=vec!["127.0.0.1:0".to_owned()];
        let mut config=WebRtcConfig::direct_first(format,bindings.clone());
        config.add_ice_server(vec!["stun:unanswered.fixture.invalid:3478".into()],String::new(),String::new()).unwrap();
        let dns=Arc::new(DnsProbe::default());let probe=Arc::clone(&dns);
        let mut video=WebRtcVideo::open_with_runtime(config,None,true,Duration::from_millis(250),move |inner|Arc::new(PendingDns::new(inner,probe))).await.unwrap();
        dns.entered.notified().await;
        let signals=video.signaling().unwrap();
        let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
        let (packets,mut received)=mpsc::channel(1);let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());
        let receiver=Arc::new(PeerConnectionBuilder::new().with_media_engine(media)
            .with_handler(Arc::new(TrickleReceiver {receiver:ReceiverEvents {packets},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake)}))
            .with_udp_addrs(bindings).build().await.unwrap());
        receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(video.offer())).unwrap()).await.unwrap();
        receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
        while !gathered.load(Ordering::Acquire) {wake.notified().await;}
        let answer=receiver.local_description().await.unwrap().sdp;let mid=field(&answer,"a=mid:").to_owned();
        for candidate in answer.lines().filter_map(|line|line.strip_prefix("a=candidate:")) {signals.add_candidate(format!("candidate:{candidate}"),mid.clone()).unwrap();}
        video.answer(strip_candidates(&answer)).await.unwrap();
        eprintln!("pending_dns_stage=answer_applied");
        let outgoing=signals.clone();let peer=Arc::clone(&receiver);
        let candidate=tokio::spawn(async move {
            loop {if let VideoIceSignal::Candidate {candidate,mid}=outgoing.next_signal().await.unwrap() {
                if candidate.contains(" typ host") {peer.add_ice_candidate(super::super::signaling::candidate(candidate,mid)).await.unwrap();return;}
            }}
        });
        assert_eq!(video.next_update().await,VideoUpdate::Connected);
        candidate.await.unwrap();
        assert!(!dns.cancelled.load(Ordering::Acquire),"direct connectivity must not require DNS to finish or be cancelled");
        assert!(!video.events.gathered.load(Ordering::Acquire),"pending DNS cannot be reported as completed gathering");
        video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,1,2,3],keyframe:true,presentation_time_micros:1_000_000}).await.unwrap();
        assert_eq!(&received.recv().await.unwrap().payload[..],&[0x65,1,2,3]);
        assert_eq!(video.route_statistics().await.route,MediaRoute::Direct);
        video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();
        assert!(dns.cancelled.load(Ordering::Acquire));
    }).await.unwrap();});
}

async fn mapped_response(socket:&UdpSocket,mapped:&str) {
    let mut bytes=[0;2000];let (length,source)=socket.recv_from(&mut bytes).await.unwrap();
    let mut request=rtc::stun::message::Message::new();request.raw=bytes[..length].to_vec();request.decode().unwrap();
    let mut response=rtc::stun::message::Message::new();response.build(&[Box::new(request.transaction_id),Box::new(rtc::stun::message::BINDING_SUCCESS),
        Box::new(rtc::stun::xoraddr::XorMappedAddress {ip:mapped.parse().unwrap(),port:40002})]).unwrap();
    socket.send_to(&response.raw,source).await.unwrap();
}

#[test]
fn stun_dns_completion_wakes_a_driver_with_all_timers_parked() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {timeout(Duration::from_secs(5),async {
        let socket=UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
        config.add_ice_server(vec!["stun:unanswered.fixture.invalid:3478".into()],String::new(),String::new()).unwrap();
        let dns=Arc::new(DnsProbe::default());let probe=Arc::clone(&dns);let gate=Arc::new(DnsGate::default());let control=Arc::clone(&gate);
        let video=WebRtcVideo::open_with_runtime(config,None,true,Duration::ZERO,move |inner|Arc::new(PendingDns {inner,probe,mode:DnsMode::Gated(control)})).await.unwrap();
        dns.entered.notified().await;
        let signals=video.signaling().unwrap();
        let VideoIceSignal::Candidate {candidate,..}=signals.next_signal().await.unwrap() else {panic!("host candidate before DNS");};
        assert!(candidate.contains(" typ host"));
        gate.wait_parked().await;
        *gate.answer.lock().unwrap()=DnsAnswer::Ready(socket.local_addr().unwrap());gate.changed.notify_waiters();
        mapped_response(&socket,"203.0.113.91").await;
        let mut reflexive=false;
        loop {match signals.next_signal().await.unwrap() {
            VideoIceSignal::Candidate {candidate,..}=>reflexive|=candidate.contains("203.0.113.91") && candidate.contains(" typ srflx"),
            VideoIceSignal::GatheringComplete=>break,
        }}
        assert!(reflexive,"DNS completion must wake progress without a protocol timer or unrelated packet");
        video.close();video.peer.close().await.unwrap();
    }).await.unwrap();});
}

#[test]
fn one_pending_stun_lookup_does_not_block_another_server() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {timeout(Duration::from_secs(5),async {
        let socket=UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
        config.add_ice_server(vec!["stun:unanswered.fixture.invalid:3478".into(),format!("stun:{}",socket.local_addr().unwrap())],String::new(),String::new()).unwrap();
        let dns=Arc::new(DnsProbe::default());let probe=Arc::clone(&dns);
        let video=WebRtcVideo::open_with_runtime(config,None,true,Duration::ZERO,move |inner|Arc::new(PendingDns::new(inner,probe))).await.unwrap();
        dns.entered.notified().await;
        mapped_response(&socket,"203.0.113.92").await;
        let signals=video.signaling().unwrap();
        loop {if let VideoIceSignal::Candidate {candidate,..}=signals.next_signal().await.unwrap() {
            if candidate.contains("203.0.113.92") && candidate.contains(" typ srflx") {break;}
        }}
        assert!(!video.events.gathered.load(Ordering::Acquire));assert!(!dns.cancelled.load(Ordering::Acquire));
        video.close();video.peer.close().await.unwrap();assert!(dns.cancelled.load(Ordering::Acquire));
    }).await.unwrap();});
}
#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_udp_after_unanswered_stun_carries_media() {late_configuration(Some(RelayTransport::Udp),true,InitialDiscovery::SilentUdp);}
#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_tcp_after_unanswered_stun_carries_media() {late_configuration(Some(RelayTransport::Tcp),true,InitialDiscovery::SilentUdp);}
#[test]
#[ignore = "requires isolated coturn TLS and a namespace blocking direct host UDP"]
fn late_relay_tls_after_unanswered_stun_carries_media() {late_configuration(Some(RelayTransport::Tls),true,InitialDiscovery::SilentUdp);}

#[test]
fn cancelling_relay_fallback_during_stun_barrier_closes_the_view() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {timeout(Duration::from_secs(5),async {
        let format=VideoFormat::H264 {profile_level_id:[66,224,31]};let bindings=vec!["127.0.0.1:0".to_owned()];
        let mut initial=WebRtcConfig::direct_first(format,bindings.clone());
        initial.add_ice_server(vec!["stun:unanswered.fixture.invalid:3478".into()],String::new(),String::new()).unwrap();
        let dns=Arc::new(DnsProbe::default());let probe=Arc::clone(&dns);
        let mut video=WebRtcVideo::open_with_runtime(initial,None,true,Duration::from_millis(250),move |inner|Arc::new(PendingDns::new(inner,probe))).await.unwrap();
        let signals=video.signaling().unwrap();
        let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
        let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());let (packets,_)=mpsc::channel(1);
        let receiver=PeerConnectionBuilder::new().with_media_engine(media)
            .with_handler(Arc::new(TrickleReceiver {receiver:ReceiverEvents {packets},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake)}))
            .with_udp_addrs(bindings.clone()).build().await.unwrap();
        receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(video.offer())).unwrap()).await.unwrap();
        receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
        while !gathered.load(Ordering::Acquire) {wake.notified().await;}
        video.answer(strip_candidates(&receiver.local_description().await.unwrap().sdp)).await.unwrap();
        dns.entered.notified().await;
        assert!(!dns.cancelled.load(Ordering::Acquire));
        let mut fallback=WebRtcConfig::direct_first(format,bindings);
        fallback.add_ice_server(vec!["stun:127.0.0.1:3478".into()],String::new(),String::new()).unwrap();
        let mut opening=Box::pin(video.begin_relay_fallback(fallback));
        let mut context=std::task::Context::from_waker(std::task::Waker::noop());
        assert!(opening.as_mut().poll(&mut context).is_pending(),"barrier must be outstanding before cancellation");
        drop(opening);
        assert!(video.ended(),"dropping startup after it begins must synchronously close authority");
        assert!(matches!(signals.next_signal().await,Err(WebRtcError::Ended)));
        video.peer.close().await.unwrap();
        assert!(dns.cancelled.load(Ordering::Acquire),"the resolver future cannot outlive peer teardown");
        assert_eq!(video.next_update().await,VideoUpdate::Ended);
        receiver.close().await.unwrap();
    }).await.unwrap();});
}

#[test]
fn late_relay_close_while_awaiting_restart_answer_stops_retries() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {timeout(Duration::from_secs(8),async {
        let format=VideoFormat::H264 {profile_level_id:[66,224,31]};let bindings=vec!["127.0.0.1:0".to_owned()];
        let mut video=WebRtcVideo::open_trickle(WebRtcConfig::direct_first(format,bindings.clone())).await.unwrap();
        let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
        let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());let (packets,_)=mpsc::channel(1);
        let receiver=PeerConnectionBuilder::new().with_media_engine(media)
            .with_handler(Arc::new(TrickleReceiver {receiver:ReceiverEvents {packets},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake)}))
            .with_udp_addrs(bindings.clone()).build().await.unwrap();
        receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(video.offer())).unwrap()).await.unwrap();
        receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
        while !gathered.load(Ordering::Acquire) {wake.notified().await;}
        video.answer(strip_candidates(&receiver.local_description().await.unwrap().sdp)).await.unwrap();
        let socket=UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut config=WebRtcConfig::direct_first(format,bindings);
        config.add_ice_server(vec![format!("turn:{}?transport=udp",socket.local_addr().unwrap())],"fixture".into(),"test-only".into()).unwrap();
        let offer=video.begin_relay_fallback(config).await.unwrap();let signals=video.signaling().unwrap();
        video.close_handle().close();
        assert_eq!(video.answer_generation(offer.generation,"unanswered".into()).await,Err(WebRtcError::Ended));
        assert!(matches!(signals.next_signal().await,Err(WebRtcError::Ended)));
        assert_eq!(video.next_update().await,VideoUpdate::Ended);
        assert_eq!(video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65],keyframe:true,presentation_time_micros:1}).await,Err(WebRtcError::Ended));
        // Drain anything queued before authority close and its two-second
        // teardown bound, then require silence through another retry window.
        let mut bytes=[0;2000];
        let _=timeout(Duration::from_millis(2200),async {while socket.recv_from(&mut bytes).await.is_ok() {}}).await;
        assert!(timeout(Duration::from_millis(300),socket.recv_from(&mut bytes)).await.is_err(),"no detached retry may survive teardown");
        receiver.close().await.unwrap();
    }).await.unwrap();});
}

fn strip_candidates(sdp:&str)->String {
    sdp.lines().filter(|line|!line.starts_with("a=candidate:") && *line!="a=end-of-candidates").collect::<Vec<_>>().join("\r\n")+"\r\n"
}
fn field<'a>(sdp:&'a str,prefix:&str)->&'a str {sdp.lines().find_map(|line|line.strip_prefix(prefix)).unwrap()}

fn late_configuration(relay:Option<RelayTransport>,restart:bool,initial:InitialDiscovery) {
    install_diagnostics();
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {timeout(Duration::from_secs(25),async {
        let bindings=if relay.is_some() {
            assert_eq!(std::env::var("VOXL_TEST_BLOCK_DIRECT_UDP").as_deref(),Ok("1"));
            for source in ["127.0.0.2","127.0.0.3"] {for target in ["127.0.0.2","127.0.0.3"] {
                let sender=UdpSocket::bind(format!("{source}:0")).await.unwrap();
                let receiver=UdpSocket::bind(format!("{target}:0")).await.unwrap();
                sender.send_to(b"direct path probe",receiver.local_addr().unwrap()).await.unwrap();
                assert!(timeout(Duration::from_millis(20),receiver.recv_from(&mut [0;64])).await.is_err(),"direct host packets must actually be dropped");
            }}
            vec!["127.0.0.2:0".to_owned(),"127.0.0.3:0".to_owned()]
        } else {vec!["127.0.0.1:0".to_owned()]};
        let format=VideoFormat::H264 {profile_level_id:[66,224,31]};
        let mut fallback=WebRtcConfig::direct_first(format,bindings.clone());
        let mut stun=None;
        let tls=if let Some(transport)=relay {
            let variable=match transport {RelayTransport::Udp=>"VOXL_TEST_TURN_UDP",RelayTransport::Tcp=>"VOXL_TEST_TURN_TCP",RelayTransport::Tls=>"VOXL_TEST_TURN_TLS",_=>unreachable!()};
            let url=std::env::var(variable).expect("fixture URL");
            assert!(rtc::ice::url::Url::parse_url(&url).unwrap().host.parse::<IpAddr>().unwrap().is_loopback());
            fallback.add_ice_server(vec![url],std::env::var("VOXL_TEST_TURN_USERNAME").expect("fixture username"),std::env::var("VOXL_TEST_TURN_PASSWORD").expect("fixture password")).unwrap();
            if transport==RelayTransport::Tls {
                let der=std::fs::read(std::env::var("VOXL_TEST_TURN_CA_DER").expect("fixture CA")).unwrap();
                let mut roots=tokio_rustls::rustls::RootCertStore::empty();roots.add(der.into()).unwrap();
                Some(Arc::new(tokio_rustls::rustls::ClientConfig::builder_with_provider(Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider())).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth()))
            } else {None}
        } else {
            let socket=UdpSocket::bind("127.0.0.1:0").await.unwrap();
            fallback.add_ice_server(vec![format!("stun:{}",socket.local_addr().unwrap())],String::new(),String::new()).unwrap();
            stun=Some(tokio::spawn(async move {
                let mut bytes=[0;2000];let (length,source)=socket.recv_from(&mut bytes).await.unwrap();
                let mut request=rtc::stun::message::Message::new();request.raw=bytes[..length].to_vec();request.decode().unwrap();
                let mut response=rtc::stun::message::Message::new();
                response.build(&[Box::new(request.transaction_id),Box::new(rtc::stun::message::BINDING_SUCCESS),
                    Box::new(rtc::stun::xoraddr::XorMappedAddress {ip:"203.0.113.19".parse().unwrap(),port:40000})]).unwrap();
                socket.send_to(&response.raw,source).await.unwrap();
            }));
            None
        };
        let mut fallback=Some(fallback);
        let mut config=if restart {WebRtcConfig::direct_first(format,bindings.clone())} else {fallback.take().unwrap()};
        let silent=if matches!(initial,InitialDiscovery::SilentUdp) {
            let socket=UdpSocket::bind("127.0.0.1:0").await.unwrap();
            config.add_ice_server(vec![format!("stun:{}",socket.local_addr().unwrap())],String::new(),String::new()).unwrap();
            Some(socket)
        } else {None};
        let dns=Arc::new(DnsProbe::default());
        if matches!(initial,InitialDiscovery::PendingDns) {
            config.add_ice_server(vec!["stun:unanswered.fixture.invalid:3478".into()],String::new(),String::new()).unwrap();
        }
        let probe=Arc::clone(&dns);
        let mut video=WebRtcVideo::open_with_runtime(config,tls,true,Duration::from_millis(250),move |inner|
            if matches!(initial,InitialDiscovery::PendingDns) {Arc::new(PendingDns::new(inner,probe))} else {inner}).await.unwrap();
        let initial_offer=video.offer().to_owned();let old_signals=video.signaling().unwrap();
        let mut media=MediaEngine::default();media.register_default_codecs().unwrap();let (packets,mut received)=mpsc::channel(4);
        let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());let connected=Arc::new(AtomicBool::new(false));let track_seen=Arc::new(AtomicBool::new(false));
        let receiver=Arc::new(PeerConnectionBuilder::new().with_media_engine(media)
            .with_handler(Arc::new(FallbackReceiver {receiver:ReceiverEvents {packets},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake),connected:Arc::clone(&connected),track_seen:Arc::clone(&track_seen)}))
            .with_udp_addrs(bindings.clone()).build().await.unwrap());
        let offer=if restart {
        receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(&initial_offer)).unwrap()).await.unwrap();
        receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
        while !gathered.load(Ordering::Acquire) {wake.notified().await;}
        let old_answer=receiver.local_description().await.unwrap().sdp;
        video.answer(strip_candidates(&old_answer)).await.unwrap();
        eprintln!("late_fallback_stage=initial_answer");
        // Neither direction has received a candidate, so no peer-reflexive
        // shortcut can disguise the late-configuration transition.
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(!video.connected());assert_eq!(old_signals.generation(),0);
        let mut invalid=WebRtcConfig::direct_first(format,vec!["127.0.0.9:0".into()]);
        invalid.add_ice_server(vec!["stun:127.0.0.1:3478".into()],String::new(),String::new()).unwrap();
        assert!(matches!(video.begin_relay_fallback(invalid).await,Err(WebRtcError::Configuration)));
        assert!(!video.ended());assert_eq!(video.signaling().unwrap().generation(),0);
        let delayed=if let Some(socket)=&silent {
            let mut bytes=[0;2000];let (length,source)=timeout(Duration::from_secs(1),socket.recv_from(&mut bytes)).await.unwrap().unwrap();
            let mut request=rtc::stun::message::Message::new();request.raw=bytes[..length].to_vec();request.decode().unwrap();
            let mut response=rtc::stun::message::Message::new();
            response.build(&[Box::new(request.transaction_id),Box::new(rtc::stun::message::BINDING_SUCCESS),
                Box::new(rtc::stun::xoraddr::XorMappedAddress {ip:"203.0.113.20".parse().unwrap(),port:40001})]).unwrap();
            Some((response.raw,source))
        } else {None};
        if matches!(initial,InitialDiscovery::PendingDns) {timeout(Duration::from_secs(1),dns.entered.notified()).await.unwrap();}
        if !matches!(initial,InitialDiscovery::Empty) {assert!(!video.events.gathered.load(Ordering::Acquire),"old discovery must still be pending");}
        let offer=timeout(Duration::from_secs(1),video.begin_relay_fallback(fallback.take().unwrap())).await.unwrap().unwrap();
        if matches!(initial,InitialDiscovery::PendingDns) {assert!(dns.cancelled.load(Ordering::Acquire),"pending DNS future must be dropped before restart is returned");}
        if let Some(socket)=&silent {
            let mut bytes=[0;2000];while socket.try_recv_from(&mut bytes).is_ok() {}
            assert!(timeout(Duration::from_millis(300),socket.recv_from(&mut bytes)).await.is_err(),"old STUN retry must stop at the acknowledged barrier");
            let (response,source)=delayed.unwrap();socket.send_to(&response,source).await.unwrap();
        }
        eprintln!("late_fallback_stage=restart_offer");
        assert_eq!(offer.generation,1);
        assert!(field(&initial_offer,"a=ice-ufrag:")!=field(&offer.sdp,"a=ice-ufrag:"),"restart must advertise fresh ICE credentials");
        assert!(field(&initial_offer,"a=fingerprint:")==field(&offer.sdp,"a=fingerprint:"),"DTLS identity must stay pinned");
        assert_eq!(old_signals.add_candidate("candidate:old".into(),"0".into()),Err(WebRtcError::Ended));
        assert!(matches!(old_signals.next_signal().await,Err(WebRtcError::Ended)));
        assert_eq!(video.answer(old_answer).await,Err(WebRtcError::Configuration));assert!(!video.ended());
        offer
        } else {VideoRestartOffer {generation:0,sdp:initial_offer}};
        let signals=video.signaling().unwrap();assert_eq!(signals.generation(),offer.generation);
        gathered.store(false,Ordering::Release);
        receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(&offer.sdp)).unwrap()).await.unwrap();
        eprintln!("late_fallback_stage=remote_offer");
        let outgoing=signals.clone();let peer=Arc::clone(&receiver);
        let candidates=tokio::spawn(async move {
            let mut discovered=false;
            loop {match outgoing.next_signal().await.unwrap() {
                VideoIceSignal::Candidate {candidate,mid}=>{
                    assert!(!candidate.contains("203.0.113.20"),"old STUN result must not enter the new generation");
                    let is_relay=candidate.contains(" typ relay");discovered|=if relay.is_some() {is_relay} else {candidate.contains(" typ srflx")};
                    // Unreachable synthetic mapped addresses are observed,
                    // never sent to the peer or used for external probes.
                    if (relay.is_some() && is_relay) || (relay.is_none() && candidate.contains(" typ host")) {
                        peer.add_ice_candidate(super::super::signaling::candidate(candidate,mid)).await.unwrap();
                    }
                },
                VideoIceSignal::GatheringComplete=>break,
            }}
            discovered
        });
        receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
        while !gathered.load(Ordering::Acquire) {wake.notified().await;}
        eprintln!("late_fallback_stage=receiver_gathered");
        let answer=receiver.local_description().await.unwrap().sdp;
        let mid=field(&answer,"a=mid:").to_owned();
        for candidate in answer.lines().filter_map(|line|line.strip_prefix("a=candidate:")) {
            signals.add_candidate(format!("candidate:{candidate}"),mid.clone()).unwrap();
        }
        video.answer_generation(offer.generation,strip_candidates(&answer)).await.unwrap();
        eprintln!("late_fallback_stage=restart_answer");
        assert_eq!(video.next_update().await,VideoUpdate::Connected);
        eprintln!("late_fallback_stage=connected");
        assert!(candidates.await.unwrap(),"new configuration must actually gather its new server candidate");
        eprintln!("late_fallback_stage=candidates_complete");
        if let Some(stun)=stun {stun.await.unwrap();}
        video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,9,8,7,6],keyframe:true,presentation_time_micros:1_000_000}).await.unwrap();
        eprintln!("late_fallback_stage=payload_written");
        let packet=match timeout(Duration::from_secs(5),received.recv()).await {
            Ok(Some(packet))=>packet,
            _=>{
                eprintln!("late fallback media deadline: receiver_connected={} track_seen={} sender={:?}",connected.load(Ordering::Acquire),track_seen.load(Ordering::Acquire),video.route_statistics().await);
                use webrtc::peer_connection::{RTCStatsReportEntry as Entry,StatsSelector};
                for entry in video.peer.get_stats(Instant::now(),StatsSelector::None).await.iter() {if let Entry::OutboundRtp(stream)=entry {eprintln!("sender RTP: bytes={}",stream.sent_rtp_stream_stats.bytes_sent);}}
                for entry in receiver.get_stats(Instant::now(),StatsSelector::None).await.iter() {match entry {
                    Entry::IceCandidatePair(pair)=>eprintln!("receiver pair: nominated={} sent={} received={}",pair.nominated,pair.bytes_sent,pair.bytes_received),
                    Entry::InboundRtp(stream)=>eprintln!("receiver RTP: bytes={}",stream.bytes_received),
                    _=>{},
                }}
                panic!("late fallback media receive deadline");
            }
        };
        assert_eq!(&packet.payload[..],&[0x65,9,8,7,6]);
        eprintln!("late_fallback_stage=payload");
        let route=video.route_statistics().await;
        assert_eq!(route.route,if relay.is_some() {MediaRoute::Relayed} else {MediaRoute::Direct});
        if let Some(transport)=relay {assert_eq!(route.local_relay_transport,transport);}
        let denied=WebRtcConfig::direct_first(format,bindings);
        assert!(matches!(video.begin_relay_fallback(denied).await,Err(WebRtcError::Configuration)));assert!(!video.ended());
        video.close();assert!(matches!(signals.next_signal().await,Err(WebRtcError::Ended)));
        receiver.close().await.unwrap();
    }).await.unwrap();});
}
