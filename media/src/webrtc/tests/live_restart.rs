//! Same-peer UDP/DTLS/SRTP across established ICE generations. Synthetic NALs,
//! not native capture/decoding, Internet recovery or credential admission.
use super::*;
use super::relay_fallback::{field,strip_candidates};
use tokio::{sync::Notify,time::timeout};

struct Receiver {
    media:ReceiverEvents,gathered:Arc<AtomicBool>,wake:Arc<Notify>,tracks:Arc<AtomicU64>,
}
#[async_trait]
impl PeerConnectionEventHandler for Receiver {
    async fn on_track(&self,track:Arc<dyn TrackRemote>) {self.tracks.fetch_add(1,Ordering::AcqRel);self.media.on_track(track).await;}
    async fn on_ice_gathering_state_change(&self,state:RTCIceGatheringState) {
        if state==RTCIceGatheringState::Complete {self.gathered.store(true,Ordering::Release);self.wake.notify_one();}
    }
}

async fn negotiate(video:&mut WebRtcVideo,receiver:Arc<dyn PeerConnection>,gathered:&AtomicBool,wake:&Notify,offer:&str,generation:u64) {
    gathered.store(false,Ordering::Release);
    receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(offer)).unwrap()).await.unwrap();
    let outgoing=video.signaling().unwrap();let target=Arc::clone(&receiver);
    let upload=tokio::spawn(async move {
        while let VideoIceSignal::Candidate {candidate,mid}=outgoing.next_signal().await.unwrap() {
            target.add_ice_candidate(super::super::signaling::candidate(candidate,mid)).await.unwrap();
        }
    });
    receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
    while !gathered.load(Ordering::Acquire) {wake.notified().await;}
    let answer=receiver.local_description().await.unwrap().sdp;
    let mid=field(&answer,"a=mid:").to_owned();let incoming=video.signaling().unwrap();
    for candidate in answer.lines().filter_map(|line|line.strip_prefix("a=candidate:")) {
        incoming.add_candidate(format!("candidate:{candidate}"),mid.clone()).unwrap();
    }
    video.answer_generation(generation,strip_candidates(&answer)).await.unwrap();
    loop {match video.next_update().await {VideoUpdate::Connected=>break,VideoUpdate::Disconnected=>{},other=>panic!("unexpected negotiation update: {other:?}")}}
    upload.await.unwrap();
}

#[test]
fn live_ice_restart_preserves_peer_identity_rtp_clock_and_fences_three_generations() {live(None);}
#[test]
#[ignore = "requires isolated coturn with two valid usernames and fixture CA"]
fn live_relay_udp_rotation_preserves_media_across_three_generations() {live(Some(RelayTransport::Udp));}
#[test]
#[ignore = "requires isolated coturn with two valid usernames and fixture CA"]
fn live_relay_tcp_rotation_preserves_media_across_three_generations() {live(Some(RelayTransport::Tcp));}
#[test]
#[ignore = "requires isolated coturn with two valid usernames and fixture CA"]
fn live_relay_tls_rotation_preserves_media_across_three_generations() {live(Some(RelayTransport::Tls));}

fn live(relay:Option<RelayTransport>) {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        timeout(Duration::from_secs(20),async {
            let config=|generation:u64| {
                let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
                if let Some(transport)=relay {
                    config.route_policy=RoutePolicy::RelayOnly;
                    let key=match transport {RelayTransport::Udp=>"VOXL_TEST_TURN_UDP",RelayTransport::Tcp=>"VOXL_TEST_TURN_TCP",RelayTransport::Tls=>"VOXL_TEST_TURN_TLS",_=>unreachable!()};
                    let url=std::env::var(key).expect("isolated fixture URL");
                    assert!(rtc::ice::url::Url::parse_url(&url).unwrap().host.parse::<std::net::IpAddr>().unwrap().is_loopback());
                    let alternate=generation%2==1;
                    let username=std::env::var(if alternate {"VOXL_TEST_TURN_ALTERNATE_USERNAME"} else {"VOXL_TEST_TURN_USERNAME"}).expect("fixture username");
                    let password=std::env::var(if alternate {"VOXL_TEST_TURN_ALTERNATE_PASSWORD"} else {"VOXL_TEST_TURN_PASSWORD"}).expect("fixture password");
                    assert_ne!(std::env::var("VOXL_TEST_TURN_USERNAME").unwrap(),std::env::var("VOXL_TEST_TURN_ALTERNATE_USERNAME").unwrap());
                    config.add_ice_server(vec![url],username,password).unwrap();
                }
                config
            };
            let tls=if relay==Some(RelayTransport::Tls) {
                let der=std::fs::read(std::env::var("VOXL_TEST_TURN_CA_DER").expect("fixture CA")).unwrap();
                let mut roots=tokio_rustls::rustls::RootCertStore::empty();roots.add(der.into()).unwrap();
                Some(Arc::new(tokio_rustls::rustls::ClientConfig::builder_with_provider(Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider())).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth()))
            } else {None};
            let mut video=WebRtcVideo::open_with_runtime(config(0),tls,true,Duration::ZERO,|runtime|runtime).await.unwrap();
            assert!(matches!(video.begin_ice_restart(config(0)).await,Err(WebRtcError::Configuration)));assert!(!video.ended());
            let initial=video.offer().to_owned();let fingerprint=field(&initial,"a=fingerprint:").to_owned();
            let mut media=MediaEngine::default();media.register_default_codecs().unwrap();let (packets,mut received)=mpsc::channel(8);
            let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());let tracks=Arc::new(AtomicU64::new(0));
            let receiver:Arc<dyn PeerConnection>=Arc::new(PeerConnectionBuilder::new().with_media_engine(media)
                .with_handler(Arc::new(Receiver {media:ReceiverEvents {packets},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake),tracks:Arc::clone(&tracks)}))
                .with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap());
            negotiate(&mut video,Arc::clone(&receiver),&gathered,&wake,&initial,0).await;
            let peer=Arc::clone(&video.peer);let ssrc=video.ssrc;
            let frame=|generation:u64|EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,9,8,generation as u8],keyframe:true,presentation_time_micros:(generation as i64+1)*1_000_000};
            video.send(frame(0)).await.unwrap();let mut previous=received.recv().await.unwrap();
            let mut ufrag=field(&initial,"a=ice-ufrag:").to_owned();
            for generation in 1..=3 {
                let old=video.signaling().unwrap();let stale_answer=receiver.local_description().await.unwrap().sdp;
                let mut invalid=config(generation);invalid.bind_addresses=vec!["127.0.0.2:0".into()];
                assert!(matches!(video.begin_ice_restart(invalid).await,Err(WebRtcError::Configuration)));
                assert!(video.connected());assert_eq!(video.signaling().unwrap().generation(),generation-1);
                let offer=video.begin_ice_restart(config(generation)).await.unwrap();
                assert_eq!(offer.generation,generation);assert!(!video.connected() && !video.ended());
                assert!(!video.ice_connected(),"the old generation's ICE reachability cannot admit the new generation");
                assert!(Arc::ptr_eq(&peer,&video.peer));assert_eq!(video.ssrc,ssrc);
                assert_eq!(field(&offer.sdp,"a=fingerprint:"),fingerprint);assert_ne!(field(&offer.sdp,"a=ice-ufrag:"),ufrag);
                ufrag=field(&offer.sdp,"a=ice-ufrag:").to_owned();
                assert_eq!(old.add_candidate("candidate:retired".into(),"0".into()),Err(WebRtcError::Ended));
                assert!(matches!(old.next_signal().await,Err(WebRtcError::Ended)));
                assert_eq!(video.answer_generation(generation-1,stale_answer).await,Err(WebRtcError::Configuration));
                assert_eq!(video.send_with_stamp(frame(generation)).await,Err(WebRtcError::NotConnected));
                assert!(matches!(video.begin_ice_restart(config(0)).await,Err(WebRtcError::Configuration)));assert!(!video.ended());
                negotiate(&mut video,Arc::clone(&receiver),&gathered,&wake,&offer.sdp,generation).await;
                assert!(video.take_keyframe_request());let stamp=video.send_with_stamp(frame(generation)).await.unwrap();
                let packet=received.recv().await.unwrap();assert_eq!(stamp.generation,generation);assert_eq!(stamp.rtp_timestamp,packet.header.timestamp);assert_eq!(&packet.payload[..],&[0x65,9,8,generation as u8]);
                assert_eq!(packet.header.ssrc,previous.header.ssrc);assert_eq!(packet.header.timestamp.wrapping_sub(previous.header.timestamp),90_000);
                assert_eq!(packet.header.sequence_number,previous.header.sequence_number.wrapping_add(1));previous=packet;
                let route=video.route_statistics().await;
                assert_eq!(route.route,if relay.is_some() {MediaRoute::Relayed} else {MediaRoute::Direct});
                if let Some(transport)=relay {assert_eq!(route.local_relay_transport,transport);}
                let (adapters,allocations)=video.closer.relays.resource_counts();
                assert_eq!(adapters,usize::from(matches!(relay,Some(RelayTransport::Tcp|RelayTransport::Tls))));
                assert_eq!(allocations,adapters,"only the current adapter allocation remains tracked");
            }
            assert_eq!(tracks.load(Ordering::Acquire),1,"ICE restart must retain the media receiver");
            // These callbacks model disconnected/recovered state, not a physical
            // packet outage. Sending is gated until connection readiness returns.
            video.events.on_connection_state_change(RTCPeerConnectionState::Disconnected).await;
            assert_eq!(video.next_update().await,VideoUpdate::Disconnected);assert!(!video.ended());
            assert_eq!(video.send(frame(4)).await,Err(WebRtcError::NotConnected));
            video.events.on_connection_state_change(RTCPeerConnectionState::Connected).await;
            assert_eq!(video.next_update().await,VideoUpdate::Connected);
            video.close();video.peer.close().await.unwrap();receiver.close().await.unwrap();
            assert!(matches!(video.begin_ice_restart(config(0)).await,Err(WebRtcError::Ended)));
        }).await.unwrap();
    });
}
