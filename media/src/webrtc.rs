//! Send-only H.264/HEVC over WebRTC using the embedder's Tokio runtime. Offers and
//! answers belong inside the authenticated application session. No signaling
//! listener, public token endpoint or process-wide runtime is created here.

use crate::{EncodedFrame, HevcProfile, MAX_FRAME_BYTES};
use crate::codec::VideoCodec;
use async_trait::async_trait;
use bytes::Bytes;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::peer_connection::configuration::RTCOfferOptions;
use rtc::rtp::codec::{h264::H264Payloader,h265::HevcPayloader};
use rtc::rtp::packetizer::Payloader;
use rtc::rtp_transceiver::rtp_sender::{RTCPFeedback, RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind};
use std::sync::{Arc, OnceLock, Mutex, atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use webrtc::media_stream::track_local::{TrackLocal, static_rtp::TrackLocalStaticRTP};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceGatheringState,RTCIceConnectionState,SettingEngineBuilder,
    RTCIceServer, RTCIceTransportPolicy, RTCPeerConnectionIceEvent, RTCPeerConnectionState, RTCSessionDescription, Registry, register_default_interceptors};
use webrtc::rtp_transceiver::{RtpSender, RTCRtpTransceiverDirection};

mod feedback;
mod stream_turn;
mod relay_interfaces;
mod signaling;
pub use stream_turn::RelayTransport;
pub use signaling::{VideoIceSignal,VideoSignaling};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRtcError { Configuration, Transport, Deadline, Ended, NotConnected, InvalidFrame }
impl std::fmt::Display for WebRtcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f,"desktop WebRTC: {self:?}") }
}
impl std::error::Error for WebRtcError {}
fn transport(_: webrtc::error::Error) -> WebRtcError { WebRtcError::Transport }

#[repr(u8)]
#[derive(Clone,Copy,PartialEq,Eq)]
enum Connection {Waiting,Connected,Disconnected,Ended}
struct Events {
    gathered: AtomicBool,
    gather_wake: Notify,
    connection: AtomicU8,
    ice: Mutex<RTCIceConnectionState>,
    changes: Arc<ChangeSignal>,
    signals: Option<signaling::Signals>,
    media_mid:OnceLock<String>,
}
impl Events {
    fn connection(&self)->Connection {match self.connection.load(Ordering::Acquire) {0=>Connection::Waiting,1=>Connection::Connected,2=>Connection::Disconnected,_=>Connection::Ended}}
    fn transition(&self,next:Connection) {
        // Terminal transport/candidate failure cannot be revived by a late
        // connection callback from the same driver.
        let mut current=self.connection.load(Ordering::Acquire);
        while current!=Connection::Ended as u8 {
            match self.connection.compare_exchange(current,next as u8,Ordering::AcqRel,Ordering::Acquire) {
                Ok(_)=>return,Err(observed)=>current=observed,
            }
        }
    }
    fn ice_connected(&self)->bool {
        self.connection()!=Connection::Ended && matches!(*self.ice.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            RTCIceConnectionState::Connected|RTCIceConnectionState::Completed)
    }
}

#[derive(Default)]
struct ChangeSignal {revision: AtomicU64,wake: Notify}
impl ChangeSignal {
    fn revision(&self) -> u64 {self.revision.load(Ordering::Acquire)}
    fn changed(&self) {
        self.revision.fetch_add(1,Ordering::AcqRel);
        self.wake.notify_waiters();
    }
    async fn wait(&self,revision: u64) {
        let notified=self.wake.notified();tokio::pin!(notified);notified.as_mut().enable();
        if self.revision()==revision {notified.await;}
    }
}
#[async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_candidate(&self,event:RTCPeerConnectionIceEvent) {
        let Some(signals)=&self.signals else {return;};
        if !event.candidate.to_json().ok().is_some_and(|candidate|signals.local(candidate.candidate,candidate.sdp_mid.unwrap_or_default()).is_ok()) {self.transition(Connection::Ended);}
        self.changes.changed();
    }
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.store(true, Ordering::Release);
            self.gather_wake.notify_one();
            self.changes.changed();
        }
    }
    async fn on_ice_connection_state_change(&self,state:RTCIceConnectionState) {
        *self.ice.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=state;
        self.changes.changed();
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        self.transition(match state {
            RTCPeerConnectionState::Connected=>Connection::Connected,
            RTCPeerConnectionState::Disconnected=>Connection::Disconnected,
            RTCPeerConnectionState::Failed|RTCPeerConnectionState::Closed=>Connection::Ended,
            _=>Connection::Waiting,
        });
        self.changes.changed();
    }
}

/// Direct host/STUN candidates remain eligible by default. Relay-only exists
/// for forced-fallback tests, not as the normal production policy.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RoutePolicy { #[default] Automatic, RelayOnly }

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MediaRoute { Direct, Relayed, #[default] Unknown }

/// Safe to log: no addresses, SDP, certificates or TURN authentication.
#[derive(Debug, Default, Clone, Copy)]
pub struct RouteStatistics {
    pub route: MediaRoute,
    pub local_relay_transport: RelayTransport,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub current_rtt: Option<Duration>,
    pub packets_discarded_on_send: u32,
}

/// Offer only the actual encoded profile, with a separate negotiated payload
/// type for that codec. AV1 needs its own OBU and RTP path before it can appear.
#[derive(Debug, Clone, Copy)]
pub enum VideoFormat { H264 { profile_level_id: [u8; 3] }, HevcMain(HevcProfile) }
impl VideoFormat {
    fn codec(self) -> VideoCodec { match self { Self::H264 {..} => VideoCodec::H264, Self::HevcMain(_) => VideoCodec::Hevc } }
    fn mime(self) -> &'static str { match self { Self::H264 {..} => "video/H264", Self::HevcMain(_) => "video/H265" } }
    fn fmtp(self) -> String {
        match self {
            Self::H264 { profile_level_id: profile } => format!("level-asymmetry-allowed=1;packetization-mode=1;profile-level-id={:02x}{:02x}{:02x}",profile[0],profile[1],profile[2]),
            Self::HevcMain(profile) => profile.fmtp(),
        }
    }
}

/// Not Debug: the supplied ICE servers contain short-lived authentication.
pub struct WebRtcConfig {
    pub ice_servers: Vec<RTCIceServer>,
    pub route_policy: RoutePolicy,
    pub bind_addresses: Vec<String>,
    pub format: VideoFormat,
}

impl WebRtcConfig {
    /// A LAN stream can open without a TURN service or credentials. Internet
    /// deployments may add STUN/TURN servers while retaining automatic routing.
    pub fn direct_first(format: VideoFormat, bind_addresses: Vec<String>) -> Self {
        Self { format, bind_addresses, ice_servers: Vec::new(), route_policy: RoutePolicy::Automatic }
    }

    /// Add authenticated relay fallback without changing the direct-route
    /// policy or requiring the embedder to depend on WebRTC's DTO types.
    pub fn add_ice_server(&mut self, urls: Vec<String>, username: String, credential: String) -> Result<(),WebRtcError> {
        if self.ice_servers.len()>=4 || urls.is_empty() || urls.len()>4 || username.len()>256 || credential.len()>512
            || urls.iter().any(|url|url.len()>512 || url.chars().any(|c|c.is_control() || c.is_whitespace() || "@/#".contains(c)))
        {return Err(WebRtcError::Configuration);}
        let server=RTCIceServer {urls,username,credential};
        server.urls().map_err(|_|WebRtcError::Configuration)?;
        self.ice_servers.push(server);
        Ok(())
    }
}

/// Synchronous authority-boundary close, usable while the sender is awaiting
/// network I/O. The single serial writer is woken and the peer is torn down on
/// its owning runtime with a separate deadline.
#[derive(Clone)]
pub struct VideoCloseHandle {
    closed: Arc<AtomicBool>,
    wake: Arc<Notify>,
    peer: Arc<dyn PeerConnection>,
    runtime: tokio::runtime::Handle,
    changes: Arc<ChangeSignal>,
    relays: Arc<stream_turn::StreamRelays>,
}
impl VideoCloseHandle {
    pub fn close(&self) {
        if self.closed.swap(true,Ordering::AcqRel) { return; }
        self.wake.notify_one();
        self.changes.changed();
        self.relays.close();
        let peer = Arc::clone(&self.peer);
        self.runtime.spawn(async move { let _ = tokio::time::timeout(Duration::from_secs(2),peer.close()).await; });
    }
}

/// Serial frame sender. Stopping is synchronous at the authority boundary;
/// native transport teardown is scheduled with a separate two-second bound.
pub struct WebRtcVideo {
    peer: Arc<dyn PeerConnection>,
    sender: Arc<dyn RtpSender>,
    track: Arc<TrackLocalStaticRTP>,
    events: Arc<Events>,
    closer: VideoCloseHandle,
    offer: String,
    negotiation: Negotiation,
    ssrc: u32,
    sequence: u16,
    timestamp: u32,
    first_pts: Option<i64>,
    last_pts: Option<i64>,
    format: VideoFormat,
    payloader: Box<dyn Payloader>,
    keyframe: Arc<AtomicBool>,
    last_keyframe_request: Option<Instant>,
    updates: UpdatePhase,
    checked_relay_failure: u64,
    checked_remote_candidates:u64,
}

#[derive(Clone)]
struct RestartSetup {bindings:Vec<String>,tls:Option<Arc<tokio_rustls::rustls::ClientConfig>>,policy:RoutePolicy}
#[derive(Clone,Copy)]
enum Startup {Discovery,Configured}
enum Negotiation {
    InitialAnswer {setup:Option<RestartSetup>,startup:Startup},
    Ready {payload:u8,generation:u64,setup:Option<RestartSetup>,startup:Startup},
    PreparingRestart,
    RestartAnswer {generation:u64,setup:RestartSetup},
}
impl Negotiation {
    fn payload(&self)->Option<u8> {match self {Self::Ready {payload,..}=>Some(*payload),_=>None}}
}
/// Carry this offer and generation inside the same authenticated view. SDP
/// and relay authentication must never be placed in diagnostics or public IPC.
pub struct VideoRestartOffer {pub generation:u64,pub sdp:String}
struct NegotiationGuard(Option<VideoCloseHandle>);
impl Drop for NegotiationGuard {fn drop(&mut self) {if let Some(close)=self.0.take() {close.close();}}}

#[derive(Clone,Copy)]
enum UpdatePhase {Waiting,Connected,Disconnected}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum VideoUpdate {Connected,Disconnected,KeyframeRequested,Ended}

struct OpeningPeer(Option<Arc<dyn PeerConnection>>, tokio::runtime::Handle);
impl Drop for OpeningPeer {
    fn drop(&mut self) {
        if let Some(peer) = self.0.take() {
            self.1.spawn(async move { let _ = tokio::time::timeout(Duration::from_secs(2),peer.close()).await; });
        }
    }
}

impl WebRtcVideo {
    pub async fn open(config: WebRtcConfig) -> Result<Self, WebRtcError> {
        Self::open_with_tls(config,None).await
    }

    /// Return SDP without waiting for server-reflexive/relay discovery. Both
    /// endpoints must exchange the signaling handle's candidates over the
    /// authenticated view; callers needing complete SDP keep using `open`.
    pub async fn open_trickle(config:WebRtcConfig)->Result<Self,WebRtcError> {
        Self::open_mode(config,None,true).await
    }

    async fn open_with_tls(config: WebRtcConfig,tls: Option<Arc<tokio_rustls::rustls::ClientConfig>>) -> Result<Self, WebRtcError> {
        Self::open_mode(config,tls,false).await
    }
    async fn open_mode(config:WebRtcConfig,tls:Option<Arc<tokio_rustls::rustls::ClientConfig>>,trickle:bool)->Result<Self,WebRtcError> {
        let turn_delay=if trickle && config.route_policy==RoutePolicy::Automatic {Duration::from_millis(250)} else {Duration::ZERO};
        Self::open_with_grace(config,tls,trickle,turn_delay).await
    }
    async fn open_with_grace(config:WebRtcConfig,tls:Option<Arc<tokio_rustls::rustls::ClientConfig>>,trickle:bool,turn_delay:Duration)->Result<Self,WebRtcError> {
        Self::open_with_runtime(config,tls,trickle,turn_delay,|runtime|runtime).await
    }
    async fn open_with_runtime<F>(mut config:WebRtcConfig,tls:Option<Arc<tokio_rustls::rustls::ClientConfig>>,trickle:bool,turn_delay:Duration,adapt_runtime:F)->Result<Self,WebRtcError>
    where F:FnOnce(Arc<dyn webrtc::runtime::Runtime>)->Arc<dyn webrtc::runtime::Runtime>+Send {
        if config.bind_addresses.is_empty() || config.bind_addresses.len() > 8 || config.ice_servers.len() > 4
            || config.bind_addresses.iter().any(|address| address.parse::<std::net::SocketAddr>().is_err())
            || matches!(config.format,VideoFormat::H264 {profile_level_id} if !matches!(profile_level_id,[66,_,_]|[100,0|12,_]))
        { return Err(WebRtcError::Configuration); }
        let discovery=trickle && config.route_policy==RoutePolicy::Automatic && config.ice_servers.iter().all(|server|
            server.username.is_empty() && server.credential.is_empty() && server.urls().ok().is_some_and(|urls|
                urls.iter().all(|url|url.scheme==rtc::ice::url::SchemeType::Stun && url.proto==rtc::ice::url::ProtoType::Udp)));
        let startup=if discovery {Startup::Discovery} else {Startup::Configured};
        let setup=trickle.then(||RestartSetup {bindings:config.bind_addresses.clone(),tls:tls.clone(),policy:config.route_policy});
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| WebRtcError::Configuration)?;
        let changes=Arc::new(ChangeSignal::default());
        let relays=Arc::new(stream_turn::StreamRelays::prepare(&mut config.ice_servers,&config.bind_addresses,tls,Arc::clone(&changes),turn_delay).await?);
        let events = Arc::new(Events { gathered: AtomicBool::new(false), gather_wake: Notify::new(), connection: AtomicU8::new(Connection::Waiting as u8),ice:Mutex::new(RTCIceConnectionState::New),changes: Arc::clone(&changes),signals:trickle.then(signaling::Signals::default),media_mid:OnceLock::new() });
        let codec = RTCRtpCodec { mime_type: config.format.mime().into(), clock_rate: 90_000, channels: 0,
            sdp_fmtp_line: config.format.fmtp(),
            rtcp_feedback: vec![RTCPFeedback { typ: "nack".into(), parameter: String::new() },
                RTCPFeedback { typ: "nack".into(), parameter: "pli".into() }, RTCPFeedback { typ: "ccm".into(), parameter: "fir".into() }],
        };
        let mut media = MediaEngine::default();
        media.register_codec(RTCRtpCodecParameters { rtp_codec: codec.clone(), payload_type: 102 },RtpCodecKind::Video).map_err(transport)?;
        let keyframe = Arc::new(AtomicBool::new(true));
        let registry = register_default_interceptors(Registry::new(),&mut media).map_err(transport)?
            .with(rtc::interceptor::Slot::from(14_000),feedback::KeyframeFeedback::new(Arc::clone(&keyframe),Arc::clone(&changes)));
        // Browser mDNS hosts commonly become peer-reflexive candidates here.
        // Proven direct connectivity must not wait the SDK's extra 0.5/1 s
        // nomination policy and lose to the embedder's shorter relay grace.
        // Deferred startup already gives direct ICE its grace before TURN is
        // installed; do not impose a second relay delay after that restart.
        let settings=SettingEngineBuilder::new()
            .with_srflx_acceptance_min_wait(Some(Duration::ZERO))
            .with_prflx_acceptance_min_wait(Some(Duration::ZERO))
            .with_relay_acceptance_min_wait((discovery || config.route_policy==RoutePolicy::RelayOnly).then_some(Duration::ZERO))
            .with_discard_local_candidates_during_ice_restart(trickle)
            .build();
        let peer: Arc<dyn PeerConnection> = Arc::new(PeerConnectionBuilder::new()
            .with_runtime(adapt_runtime(relays.runtime()))
            .with_setting_engine(settings)
            .with_configuration(RTCConfigurationBuilder::new().with_ice_servers(config.ice_servers)
                .with_ice_transport_policy(if config.route_policy == RoutePolicy::RelayOnly { RTCIceTransportPolicy::Relay } else { RTCIceTransportPolicy::All }).build())
            .with_media_engine(media).with_interceptor_registry(registry).with_handler(Arc::<Events>::clone(&events))
            .with_udp_addrs(config.bind_addresses).build().await.map_err(transport)?);
        let mut guard = OpeningPeer(Some(Arc::clone(&peer)),runtime.clone());
        let ssrc = rand::random::<u32>();
        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new("desktop".into(),"screen".into(),"Desktop".into(),RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters { rtp_coding_parameters: RTCRtpCodingParameters { ssrc: Some(ssrc), ..Default::default() },codec,..Default::default() }])));
        let sender = peer.add_track(Arc::<TrackLocalStaticRTP>::clone(&track) as Arc<dyn TrackLocal>).await.map_err(transport)?;
        for transceiver in peer.get_transceivers().await { transceiver.set_direction(RTCRtpTransceiverDirection::Sendonly).await.map_err(transport)?; }
        let offer = peer.create_offer(None).await.map_err(transport)?;
        peer.set_local_description(offer).await.map_err(transport)?;
        if !trickle {tokio::time::timeout(Duration::from_secs(6),async {
            while !events.gathered.load(Ordering::Acquire) { events.gather_wake.notified().await; }
        }).await.map_err(|_| WebRtcError::Deadline)?;}
        let mut offer = peer.local_description().await.ok_or(WebRtcError::Transport)?.sdp;
        if trickle {
            offer=offer.lines().filter(|line|*line!="a=end-of-candidates").collect::<Vec<_>>().join("\r\n")+"\r\n";
            let mid=offer.lines().find_map(|line|line.strip_prefix("a=mid:")).filter(|mid|!mid.is_empty() && mid.len()<=256).ok_or(WebRtcError::Configuration)?;
            let _=events.media_mid.set(mid.to_owned());
            // The wrapper now exchanges incremental candidates. Advertise that
            // application capability in the one video media section; transport,
            // fingerprints, ICE credentials and the SDK's local SDP stay intact.
            if !offer.lines().any(|line|line.strip_prefix("a=ice-options:").is_some_and(|options|options.split_whitespace().any(|option|option=="trickle"))) {
                if !offer.ends_with('\n') {offer.push_str("\r\n");}
                offer.push_str("a=ice-options:trickle\r\n");
            }
        }
        if offer.is_empty() || offer.len() > 256 * 1024 { return Err(WebRtcError::Configuration); }
        guard.0 = None;
        let closer = VideoCloseHandle { closed: Arc::new(AtomicBool::new(false)),wake: Arc::new(Notify::new()),peer: Arc::clone(&peer),runtime,changes,relays };
        Ok(Self { peer,sender,track,events,closer,offer,negotiation:Negotiation::InitialAnswer {setup,startup},ssrc,
            sequence: rand::random(),timestamp: rand::random(),first_pts: None,last_pts: None,format: config.format,payloader: match config.format {
                VideoFormat::H264 {..} => Box::<H264Payloader>::default(), VideoFormat::HevcMain(_) => Box::<HevcPayloader>::default() },
            keyframe,last_keyframe_request: None,updates: UpdatePhase::Waiting,checked_relay_failure:0,checked_remote_candidates:0 })
    }

    pub fn offer(&self) -> &str { &self.offer }
    pub fn signaling(&self)->Option<VideoSignaling> {
        self.events.signals.as_ref().map(|signals|VideoSignaling {events:Arc::clone(&self.events),closer:self.closer.clone(),generation:signals.generation()})
    }
    pub fn connected(&self) -> bool { !self.ended() && self.negotiation.payload().is_some() && self.events.connection()==Connection::Connected }
    /// ICE reachability is available before DTLS and the sender transport.
    /// Use it to retire optional startup relay work, never to permit RTP/input.
    pub fn ice_connected(&self)->bool {!self.ended() && self.events.ice_connected()}
    pub fn ended(&self) -> bool { self.closer.closed.load(Ordering::Acquire) || self.events.connection()==Connection::Ended }
    /// Wait without polling for first connection, coalesced keyframe feedback
    /// or terminal close. A pending rate-limited PLI has one deadline; idle
    /// connected media has no periodic timer. Cancelling a wait loses no state.
    pub async fn next_update(&mut self) -> VideoUpdate {
        loop {
            let revision=self.events.changes.revision();
            if self.ended() {return VideoUpdate::Ended;}
            if self.negotiation.payload().is_some() && let Some(signals)=&self.events.signals {
                let changed=signals.remote_revision();
                if changed!=self.checked_remote_candidates {
                    self.checked_remote_candidates=changed;
                    for (candidate,mid) in signals.take_remote() {
                        if self.peer.add_ice_candidate(signaling::candidate(candidate,mid)).await.is_err() {
                            self.close();return VideoUpdate::Ended;
                        }
                    }
                }
            }
            let failure_revision=self.closer.relays.failure_revision();
            if failure_revision!=self.checked_relay_failure
                && let Ok(Some(transport))=self.sender.transport().await
                && let Ok(Some(pair))=transport.ice_transport().get_selected_candidate_pair().await {
                let local=pair.local();
                if local.address.parse::<std::net::IpAddr>().ok().is_some_and(|ip|self.closer.relays.is_failed(std::net::SocketAddr::new(ip,local.port))) {
                    self.close();return VideoUpdate::Ended;
                }
                self.checked_relay_failure=failure_revision;
            }
            let connected=self.connected();
            if self.events.connection()==Connection::Disconnected && !matches!(self.updates,UpdatePhase::Disconnected) {
                self.updates=UpdatePhase::Disconnected;return VideoUpdate::Disconnected;
            }
            if connected && !matches!(self.updates,UpdatePhase::Connected) {
                self.updates=UpdatePhase::Connected;
                return VideoUpdate::Connected;
            }
            if connected && self.take_keyframe_request() {return VideoUpdate::KeyframeRequested;}
            let deadline=if connected && self.keyframe.load(Ordering::Acquire) {
                self.last_keyframe_request.map(|last|last+Duration::from_millis(250))
            } else {None};
            tokio::select! {
                _ = self.events.changes.wait(revision) => {},
                _ = async {
                    if let Some(deadline)=deadline {tokio::time::sleep_until(deadline.into()).await;}
                    else {std::future::pending::<()>().await;}
                } => {},
            }
        }
    }
    pub fn take_keyframe_request(&mut self) -> bool {
        if self.last_keyframe_request.is_some_and(|time| time.elapsed() < Duration::from_millis(250)) { return false; }
        if self.keyframe.swap(false,Ordering::AcqRel) { self.last_keyframe_request = Some(Instant::now()); true } else { false }
    }

    /// Read the nominated path rather than inferring it from configured ICE
    /// servers. A TURN URL existing does not mean video actually used TURN.
    pub async fn route_statistics(&self) -> RouteStatistics {
        use webrtc::peer_connection::{RTCStatsReportEntry as Entry, StatsSelector};
        use rtc::peer_connection::transport::RTCIceCandidateType;
        if self.ended() { return RouteStatistics::default(); }
        let Ok(Some(transport)) = self.sender.transport().await else { return RouteStatistics::default(); };
        let Ok(Some(selected)) = transport.ice_transport().get_selected_candidate_pair().await else { return RouteStatistics::default(); };
        let local = selected.local();
        let remote = selected.remote();
        let route = if local.typ == RTCIceCandidateType::Relay || remote.typ == RTCIceCandidateType::Relay {
            MediaRoute::Relayed
        } else if local.typ == RTCIceCandidateType::Unspecified || remote.typ == RTCIceCandidateType::Unspecified {
            MediaRoute::Unknown
        } else { MediaRoute::Direct };
        let local_relay_transport=if local.typ==RTCIceCandidateType::Relay {
            local.address.parse::<std::net::IpAddr>().ok().map(|ip|self.closer.relays.transport(std::net::SocketAddr::new(ip,local.port)))
                .filter(|value|*value!=RelayTransport::Unknown).unwrap_or(RelayTransport::Udp)
        } else {RelayTransport::Unknown};
        // The transport's selected pair is authoritative. Statistics references
        // in rtc 0.21 can be absent even after media flows; join counters using
        // the selected candidates rather than treating a missing stats link as
        // evidence that the route itself is unknown.
        let report = self.peer.get_stats(Instant::now(), StatsSelector::None).await;
        for entry in report.iter() {
            let Entry::IceCandidatePair(pair) = entry else { continue; };
            if pair.local_candidate_id != local.id || pair.remote_candidate_id != remote.id { continue; }
            return RouteStatistics { route,local_relay_transport,bytes_sent: pair.bytes_sent,bytes_received: pair.bytes_received,
                current_rtt: Duration::try_from_secs_f64(pair.current_round_trip_time).ok().filter(|time| !time.is_zero()),
                packets_discarded_on_send: pair.packets_discarded_on_send };
        }
        RouteStatistics { route,local_relay_transport,..Default::default() }
    }

    pub async fn answer(&mut self, sdp: String) -> Result<(), WebRtcError> {
        self.answer_generation(0,sdp).await
    }

    pub async fn answer_generation(&mut self,generation:u64,sdp:String)->Result<(),WebRtcError> {
        if self.ended() {return Err(WebRtcError::Ended);}
        let (setup,startup)=match &self.negotiation {
            Negotiation::InitialAnswer {setup,startup} if generation==0=>(setup.clone(),*startup),
            Negotiation::RestartAnswer {generation:expected,setup} if generation==*expected=>(Some(setup.clone()),Startup::Configured),
            _=>return Err(WebRtcError::Configuration),
        };
        if sdp.is_empty() || sdp.len() > 256 * 1024 { return Err(WebRtcError::Configuration); }
        let mut guard=NegotiationGuard(Some(self.closer.clone()));
        self.peer.set_remote_description(RTCSessionDescription::answer(sdp).map_err(transport)?).await.map_err(transport)?;
        let payload = self.sender.get_parameters().await.map_err(transport)?.rtp_parameters.codecs.iter()
            .find(|codec| codec.rtp_codec.mime_type.eq_ignore_ascii_case(self.format.mime())).map(|codec| codec.payload_type);
        let Some(payload)=payload else {return Err(WebRtcError::Transport);};
        if self.ended() {return Err(WebRtcError::Ended);}
        self.negotiation=Negotiation::Ready {payload,generation,setup,startup};
        guard.0=None;
        Ok(())
    }

    /// One startup fallback for an initially STUN-only/server-free Automatic trickle
    /// peer. The embedder first gives direct ICE a grace period, then fetches
    /// short-lived credentials and calls this only while still unconnected.
    /// It must stop old signaling, authenticate the new offer/generation and
    /// replace its signaling handle. No codec, bindings, policy or DTLS identity
    /// may change. Use `begin_ice_restart` for an established trickle peer.
    pub async fn begin_relay_fallback(&mut self,mut config:WebRtcConfig)->Result<VideoRestartOffer,WebRtcError> {
        tokio::time::timeout(Duration::from_secs(6),self.relay_fallback(&mut config)).await.map_err(|_|WebRtcError::Deadline)?
    }
    /// Begin a serialized live ICE restart on the same peer/DTLS identity.
    /// The owner must release control, mark media reconnecting and authenticate
    /// the new generation/answer/candidates. Call `next_update` before resuming
    /// video and send a fresh keyframe. This method grants no application rights.
    /// Only unchanged explicit ephemeral bindings, codec and route policy are
    /// accepted; interface replacement and retry/credential schedules belong to
    /// the owner. Old allocations/adapters are retired, not accumulated.
    pub async fn begin_ice_restart(&mut self,mut config:WebRtcConfig)->Result<VideoRestartOffer,WebRtcError> {
        tokio::time::timeout(Duration::from_secs(6),self.restart(&mut config,false)).await.map_err(|_|WebRtcError::Deadline)?
    }
    async fn relay_fallback(&mut self,config:&mut WebRtcConfig)->Result<VideoRestartOffer,WebRtcError> {self.restart(config,true).await}
    async fn restart(&mut self,config:&mut WebRtcConfig,startup_only:bool)->Result<VideoRestartOffer,WebRtcError> {
        if self.ended() {return Err(WebRtcError::Ended);}
        let (setup,payload,generation,startup)=match &self.negotiation {
            Negotiation::Ready {setup:Some(setup),payload,generation,startup}=>(setup.clone(),*payload,*generation,*startup),
            _=>return Err(WebRtcError::Configuration),
        };
        if startup_only && (generation!=0 || !matches!(startup,Startup::Discovery) || self.connected() || self.ice_connected()) {return Err(WebRtcError::Configuration);}
        let next=generation.checked_add(1).filter(|next|*next<=u64::from(u32::MAX)).ok_or(WebRtcError::Configuration)?;
        if config.route_policy!=setup.policy || config.bind_addresses!=setup.bindings
            || config.format.mime()!=self.format.mime() || config.format.fmtp()!=self.format.fmtp()
            || config.ice_servers.len()>4 || (startup_only && config.ice_servers.is_empty())
            || (!startup_only && setup.bindings.iter().any(|binding|binding.parse::<std::net::SocketAddr>().map_or(true,|address|address.port()!=0)))
        {return Err(WebRtcError::Configuration);}
        let mut validated=WebRtcConfig::direct_first(config.format,setup.bindings.clone());
        for server in &config.ice_servers {validated.add_ice_server(server.urls.clone(),server.username.clone(),server.credential.clone())?;}
        let mut guard=NegotiationGuard(Some(self.closer.clone()));
        self.negotiation=Negotiation::PreparingRestart;
        if startup_only {self.peer.quiesce_stun_gathering_for_restart().await.map_err(transport)?;}
        else {self.peer.quiesce_ice_gathering_for_restart().await.map_err(transport)?;}
        if self.ended() {return Err(WebRtcError::Ended);}
        if startup_only && (self.events.connection()==Connection::Connected || self.ice_connected()) {
            self.negotiation=Negotiation::Ready {payload,generation,setup:Some(setup),startup};
            guard.0=None;return Err(WebRtcError::Configuration);
        }
        if !startup_only {
            // The acknowledged barrier has delivered the old callbacks. Do not
            // carry their reachability/DTLS-ready observations into this epoch;
            // fresh driver callbacks must establish the replacement route.
            self.events.transition(Connection::Waiting);
            *self.events.ice.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=RTCIceConnectionState::New;
        }
        self.events.gathered.store(false,Ordering::Release);
        let signals=self.events.signals.as_ref().ok_or(WebRtcError::Configuration)?;
        signals.advance();self.events.changes.changed();
        if startup_only {self.closer.relays.add_fallback(&mut config.ice_servers,&setup.bindings,setup.tls.clone(),Arc::clone(&self.events.changes)).await?;}
        else {self.closer.relays.replace(&mut config.ice_servers,&setup.bindings,setup.tls.clone(),Arc::clone(&self.events.changes)).await?;}
        let current=self.peer.get_configuration().await;
        let configuration=RTCConfigurationBuilder::new().with_ice_servers(std::mem::take(&mut config.ice_servers))
            .with_ice_transport_policy(current.ice_transport_policy()).with_bundle_policy(current.bundle_policy())
            .with_rtcp_mux_policy(current.rtcp_mux_policy()).with_peer_identitys(current.peer_identity().to_owned())
            .with_certificates(current.certificates().to_vec()).with_ice_candidate_pool_size(current.ice_candidate_pool_size()).build();
        self.peer.set_configuration(configuration).await.map_err(transport)?;
        let offer=self.peer.create_offer(Some(RTCOfferOptions {ice_restart:true})).await.map_err(transport)?;
        self.peer.set_local_description(offer).await.map_err(transport)?;
        let sdp=self.peer.local_description().await.ok_or(WebRtcError::Transport)?.sdp;
        let mut sdp=sdp.lines().filter(|line|*line!="a=end-of-candidates").collect::<Vec<_>>().join("\r\n")+"\r\n";
        if !sdp.lines().any(|line|line.strip_prefix("a=ice-options:").is_some_and(|options|options.split_whitespace().any(|option|option=="trickle"))) {sdp.push_str("a=ice-options:trickle\r\n");}
        if self.ended() {return Err(WebRtcError::Ended);}
        if sdp.is_empty() || sdp.len()>256*1024 {return Err(WebRtcError::Configuration);}
        self.offer=sdp.clone();self.negotiation=Negotiation::RestartAnswer {generation:next,setup};self.updates=UpdatePhase::Waiting;
        self.keyframe.store(true,Ordering::Release);self.last_keyframe_request=None;
        guard.0=None;Ok(VideoRestartOffer {generation:next,sdp})
    }

    pub async fn send(&mut self, frame: EncodedFrame) -> Result<(), WebRtcError> {
        if self.ended() { return Err(WebRtcError::Ended); }
        if !self.connected() { return Err(WebRtcError::NotConnected); }
        let payload_type = self.negotiation.payload().ok_or(WebRtcError::NotConnected)?;
        if frame.codec != self.format.codec() || frame.annex_b.is_empty() || frame.annex_b.len() > MAX_FRAME_BYTES || frame.presentation_time_micros < 0
            || self.last_pts.is_some_and(|last| frame.presentation_time_micros <= last)
        { return Err(WebRtcError::InvalidFrame); }
        if self.first_pts.is_none() && !frame.keyframe { return Err(WebRtcError::InvalidFrame); }
        let first_pts = *self.first_pts.get_or_insert(frame.presentation_time_micros);
        let ticks = (i128::from(frame.presentation_time_micros)-i128::from(first_pts))*90_000/1_000_000;
        let timestamp = self.timestamp.wrapping_add(ticks as u32);
        let payloads = self.payloader.payload(1100,&Bytes::from(frame.annex_b)).map_err(transport)?;
        if payloads.is_empty() { self.close(); return Err(WebRtcError::InvalidFrame); }
        let last = payloads.len() - 1;
        let result = tokio::time::timeout(Duration::from_millis(200),async {
            for (index,payload) in payloads.into_iter().enumerate() {
                let packet = rtc::rtp::Packet { header: rtc::rtp::header::Header { version: 2, marker: index == last,
                    payload_type,sequence_number: self.sequence,timestamp,ssrc: self.ssrc,..Default::default() },payload };
                self.sequence = self.sequence.wrapping_add(1);
                if self.ended() { return Err(WebRtcError::Ended); }
                tokio::select! {
                    biased;
                    _ = self.closer.wake.notified() => return Err(WebRtcError::Ended),
                    result = self.track.write_rtp(packet) => { result.map_err(transport)?; }
                }
            }
            Ok::<_,WebRtcError>(())
        }).await.map_err(|_| WebRtcError::Deadline).and_then(|result| result);
        if result.is_err() { self.close(); }
        self.last_pts = Some(frame.presentation_time_micros);
        result
    }

    pub fn close_handle(&self) -> VideoCloseHandle { self.closer.clone() }
    pub fn close(&self) { self.closer.close(); }
}
impl Drop for WebRtcVideo { fn drop(&mut self) { self.close(); } }

#[cfg(test)]
mod tests;
