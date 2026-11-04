//! Send-only H.264/HEVC over WebRTC using the embedder's Tokio runtime. Offers and
//! answers belong inside the authenticated application session. No signaling
//! listener, public token endpoint or process-wide runtime is created here.

use crate::{EncodedFrame, HevcProfile, MAX_FRAME_BYTES};
use crate::codec::VideoCodec;
use async_trait::async_trait;
use bytes::Bytes;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::rtp::codec::{h264::H264Payloader,h265::HevcPayloader};
use rtc::rtp::packetizer::Payloader;
use rtc::rtp_transceiver::rtp_sender::{RTCPFeedback, RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use webrtc::media_stream::track_local::{TrackLocal, static_rtp::TrackLocalStaticRTP};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceGatheringState,
    RTCIceServer, RTCIceTransportPolicy, RTCPeerConnectionState, RTCSessionDescription, Registry, register_default_interceptors};
use webrtc::rtp_transceiver::{RtpSender, RTCRtpTransceiverDirection};

mod feedback;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRtcError { Configuration, Transport, Deadline, Ended, NotConnected, InvalidFrame }
impl std::fmt::Display for WebRtcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f,"desktop WebRTC: {self:?}") }
}
impl std::error::Error for WebRtcError {}
fn transport(_: webrtc::error::Error) -> WebRtcError { WebRtcError::Transport }

struct Events {
    gathered: AtomicBool,
    gather_wake: Notify,
    connected: AtomicBool,
    ended: AtomicBool,
}
#[async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.store(true, Ordering::Release);
            self.gather_wake.notify_one();
        }
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        self.connected.store(state == RTCPeerConnectionState::Connected, Ordering::Release);
        if matches!(state, RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Closed) {
            self.ended.store(true, Ordering::Release);
        }
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
}
impl VideoCloseHandle {
    pub fn close(&self) {
        if self.closed.swap(true,Ordering::AcqRel) { return; }
        self.wake.notify_one();
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
    payload_type: Option<u8>,
    ssrc: u32,
    sequence: u16,
    timestamp: u32,
    first_pts: Option<i64>,
    last_pts: Option<i64>,
    format: VideoFormat,
    payloader: Box<dyn Payloader>,
    keyframe: Arc<AtomicBool>,
    last_keyframe_request: Option<Instant>,
}

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
        if config.bind_addresses.is_empty() || config.bind_addresses.len() > 8 || config.ice_servers.len() > 4
            || config.bind_addresses.iter().any(|address| address.parse::<std::net::SocketAddr>().is_err())
            || matches!(config.format,VideoFormat::H264 {profile_level_id} if profile_level_id[0] != 66)
        { return Err(WebRtcError::Configuration); }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| WebRtcError::Configuration)?;
        let events = Arc::new(Events { gathered: AtomicBool::new(false), gather_wake: Notify::new(), connected: AtomicBool::new(false), ended: AtomicBool::new(false) });
        let codec = RTCRtpCodec { mime_type: config.format.mime().into(), clock_rate: 90_000, channels: 0,
            sdp_fmtp_line: config.format.fmtp(),
            rtcp_feedback: vec![RTCPFeedback { typ: "nack".into(), parameter: String::new() },
                RTCPFeedback { typ: "nack".into(), parameter: "pli".into() }, RTCPFeedback { typ: "ccm".into(), parameter: "fir".into() }],
        };
        let mut media = MediaEngine::default();
        media.register_codec(RTCRtpCodecParameters { rtp_codec: codec.clone(), payload_type: 102 },RtpCodecKind::Video).map_err(transport)?;
        let keyframe = Arc::new(AtomicBool::new(true));
        let registry = register_default_interceptors(Registry::new(),&mut media).map_err(transport)?
            .with(rtc::interceptor::Slot::from(14_000),feedback::KeyframeFeedback::new(Arc::clone(&keyframe)));
        let peer: Arc<dyn PeerConnection> = Arc::new(PeerConnectionBuilder::new()
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
        tokio::time::timeout(Duration::from_secs(6),async {
            while !events.gathered.load(Ordering::Acquire) { events.gather_wake.notified().await; }
        }).await.map_err(|_| WebRtcError::Deadline)?;
        let offer = peer.local_description().await.ok_or(WebRtcError::Transport)?.sdp;
        if offer.is_empty() || offer.len() > 256 * 1024 { return Err(WebRtcError::Configuration); }
        guard.0 = None;
        let closer = VideoCloseHandle { closed: Arc::new(AtomicBool::new(false)),wake: Arc::new(Notify::new()),peer: Arc::clone(&peer),runtime };
        Ok(Self { peer,sender,track,events,closer,offer,payload_type: None,ssrc,
            sequence: rand::random(),timestamp: rand::random(),first_pts: None,last_pts: None,format: config.format,payloader: match config.format {
                VideoFormat::H264 {..} => Box::<H264Payloader>::default(), VideoFormat::HevcMain(_) => Box::<HevcPayloader>::default() },
            keyframe,last_keyframe_request: None })
    }

    pub fn offer(&self) -> &str { &self.offer }
    pub fn connected(&self) -> bool { !self.ended() && self.events.connected.load(Ordering::Acquire) }
    pub fn ended(&self) -> bool { self.closer.closed.load(Ordering::Acquire) || self.events.ended.load(Ordering::Acquire) }
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
        // The transport's selected pair is authoritative. Statistics references
        // in rtc 0.21 can be absent even after media flows; join counters using
        // the selected candidates rather than treating a missing stats link as
        // evidence that the route itself is unknown.
        let report = self.peer.get_stats(Instant::now(), StatsSelector::None).await;
        for entry in report.iter() {
            let Entry::IceCandidatePair(pair) = entry else { continue; };
            if pair.local_candidate_id != local.id || pair.remote_candidate_id != remote.id { continue; }
            return RouteStatistics { route,bytes_sent: pair.bytes_sent,bytes_received: pair.bytes_received,
                current_rtt: Duration::try_from_secs_f64(pair.current_round_trip_time).ok().filter(|time| !time.is_zero()),
                packets_discarded_on_send: pair.packets_discarded_on_send };
        }
        RouteStatistics { route,..Default::default() }
    }

    pub async fn answer(&mut self, sdp: String) -> Result<(), WebRtcError> {
        if self.ended() || self.payload_type.is_some() { return Err(WebRtcError::Ended); }
        if sdp.is_empty() || sdp.len() > 256 * 1024 { return Err(WebRtcError::Configuration); }
        self.peer.set_remote_description(RTCSessionDescription::answer(sdp).map_err(transport)?).await.map_err(transport)?;
        self.payload_type = self.sender.get_parameters().await.map_err(transport)?.rtp_parameters.codecs.iter()
            .find(|codec| codec.rtp_codec.mime_type.eq_ignore_ascii_case(self.format.mime())).map(|codec| codec.payload_type);
        if self.payload_type.is_none() { self.close(); return Err(WebRtcError::Transport); }
        Ok(())
    }

    pub async fn send(&mut self, frame: EncodedFrame) -> Result<(), WebRtcError> {
        if self.ended() { return Err(WebRtcError::Ended); }
        if !self.connected() { return Err(WebRtcError::NotConnected); }
        let payload_type = self.payload_type.ok_or(WebRtcError::NotConnected)?;
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
