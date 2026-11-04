//! Exercise the real UDP/DTLS/SRTP path without a display, OS input or decoder.
//! Payloads are synthetic NAL units: this is transport, not H.264 quality proof.
use super::*;
use webrtc::media_stream::track_remote::{TrackRemote,TrackRemoteEvent};
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use tokio::sync::mpsc;

#[test]
fn webrtc_change_wait_survives_cancellation_and_prior_notifications() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let changes=ChangeSignal::default();
        let first=changes.revision();
        changes.changed();
        tokio::time::timeout(Duration::from_millis(50),changes.wait(first)).await.unwrap();
        let second=changes.revision();
        assert!(tokio::time::timeout(Duration::from_millis(5),changes.wait(second)).await.is_err());
        changes.changed();
        tokio::time::timeout(Duration::from_millis(50),changes.wait(second)).await.unwrap();
    });
}

#[test]
fn webrtc_relay_configuration_preserves_direct_policy_and_bounds_credentials() {
    let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id: [66,224,31]},vec!["127.0.0.1:0".into()]);
    assert!(config.add_ice_server(vec!["turn:relay.example.com:3478?transport=udp".into()],"user".into(),"password".into()).is_ok());
    assert_eq!(config.route_policy,RoutePolicy::Automatic);
    assert_eq!(config.ice_servers.len(),1);
    for (url,user,password) in [("https://relay.example.com","user","password"),("turn:relay.example.com","",""),
        ("turn:user@relay.example.com","user","password")] {
        assert_eq!(config.add_ice_server(vec![url.into()],user.into(),password.into()),Err(WebRtcError::Configuration));
        assert_eq!(config.ice_servers.len(),1);
    }
    assert_eq!(config.add_ice_server(vec!["turn:relay.example.com".into()],"x".repeat(257),"password".into()),Err(WebRtcError::Configuration));
    for _ in 1..4 {
        config.add_ice_server(vec!["turns:relay.example.com:443?transport=tcp".into()],"user".into(),"password".into()).unwrap();
    }
    assert_eq!(config.add_ice_server(vec!["stun:relay.example.com".into()],String::new(),String::new()),Err(WebRtcError::Configuration));
}

#[test]
fn webrtc_explicit_stun_discovers_a_reflexive_candidate_and_keeps_host_candidates() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10),async {
            let socket=tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let address=socket.local_addr().unwrap();
            // A real UDP STUN exchange with a synthetic mapped address. This
            // proves candidate gathering, not physical NAT traversal or media.
            let server=tokio::spawn(async move {
                let mut buffer=[0u8;2000];
                let (length,source)=socket.recv_from(&mut buffer).await.unwrap();
                let mut request=rtc::stun::message::Message::new();
                request.raw=buffer[..length].to_vec();request.decode().unwrap();
                assert_eq!(request.typ,rtc::stun::message::BINDING_REQUEST);
                let response={
                    let mut response=rtc::stun::message::Message::new();
                    let setters:Vec<Box<dyn rtc::stun::message::Setter>>=vec![Box::new(request.transaction_id),
                        Box::new(rtc::stun::message::BINDING_SUCCESS),Box::new(rtc::stun::xoraddr::XorMappedAddress {
                            ip:"203.0.113.19".parse().unwrap(),port:40000})];
                    response.build(&setters).unwrap();response
                };
                socket.send_to(&response.raw,source).await.unwrap();
            });
            let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
            config.add_ice_server(vec![format!("stun:{address}")],String::new(),String::new()).unwrap();
            assert_eq!(config.route_policy,RoutePolicy::Automatic);
            let video=WebRtcVideo::open(config).await.unwrap();
            server.await.unwrap();
            assert!(video.offer().contains("203.0.113.19 40000 typ srflx"),"STUN must publish the mapped address");
            assert!(video.offer().lines().any(|line|line.contains("127.0.0.1") && line.contains("typ host")),"STUN must not remove the LAN route");
            assert!(!video.offer().contains("typ relay"));
            video.close();assert!(video.ended());
        }).await.unwrap();
    });
}

struct ReceiverEvents { packets: mpsc::Sender<rtc::rtp::Packet> }
#[async_trait]
impl PeerConnectionEventHandler for ReceiverEvents {
    async fn on_track(&self,track: Arc<dyn TrackRemote>) {
        let packets = self.packets.clone();
        tokio::spawn(async move {
            while let Some(event) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(packet) = event {
                    let ssrc = packet.header.ssrc;
                    if packets.try_send(packet).is_err() { break; }
                    let _ = track.write_rtcp(vec![Box::new(PictureLossIndication { sender_ssrc: 7,media_ssrc: ssrc })]).await;
                }
            }
        });
    }
}

#[test]
fn trickle_direct_media_precedes_held_stun_and_applies_early_remote_candidates() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(20),async {
            let socket=tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();let address=socket.local_addr().unwrap();
            let (release,held)=tokio::sync::oneshot::channel();
            let stun=tokio::spawn(async move {
                let mut bytes=[0;2000];let (length,source)=socket.recv_from(&mut bytes).await.unwrap();
                let mut request=rtc::stun::message::Message::new();request.raw=bytes[..length].to_vec();request.decode().unwrap();
                held.await.unwrap();
                let response={let mut response=rtc::stun::message::Message::new();let setters:Vec<Box<dyn rtc::stun::message::Setter>>=vec![Box::new(request.transaction_id),Box::new(rtc::stun::message::BINDING_SUCCESS),Box::new(rtc::stun::xoraddr::XorMappedAddress {ip:"203.0.113.19".parse().unwrap(),port:40000})];response.build(&setters).unwrap();response};
                socket.send_to(&response.raw,source).await.unwrap();
            });
            let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
            config.add_ice_server(vec![format!("stun:{address}")],String::new(),String::new()).unwrap();
            let mut video=tokio::time::timeout(Duration::from_secs(1),WebRtcVideo::open_trickle(config)).await.expect("offer cannot wait for held STUN").unwrap();
            assert!(!video.events.gathered.load(Ordering::Acquire));
            assert!(video.offer().contains("a=ice-options:trickle"));assert!(!video.offer().contains("a=end-of-candidates"));
            let signals=video.signaling().unwrap();
            let mut media=MediaEngine::default();media.register_default_codecs().unwrap();let (sender,mut packets)=mpsc::channel(4);
            let receiver=PeerConnectionBuilder::new().with_media_engine(media).with_handler(Arc::new(ReceiverEvents {packets:sender})).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap();
            receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
            receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
            let answer=receiver.local_description().await.unwrap().sdp;
            let mid=answer.lines().find_map(|line|line.strip_prefix("a=mid:")).unwrap().to_owned();
            for candidate in answer.lines().filter_map(|line|line.strip_prefix("a=candidate:")) {signals.add_candidate(format!("candidate:{candidate}"),mid.clone()).unwrap();}
            assert!(video.events.signals.as_ref().unwrap().remote_revision()>0);
            let answer=answer.lines().filter(|line|!line.starts_with("a=candidate:") && *line!="a=end-of-candidates").collect::<Vec<_>>().join("\r\n")+"\r\n";
            video.answer(answer).await.unwrap();assert_eq!(video.next_update().await,VideoUpdate::Connected);
            assert!(video.events.signals.as_ref().unwrap().take_remote().is_empty());
            video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,9,8,7,6],keyframe:true,presentation_time_micros:1_000_000}).await.unwrap();
            assert_eq!(&packets.recv().await.unwrap().payload[..],&[0x65,9,8,7,6]);
            assert!(!video.events.gathered.load(Ordering::Acquire),"direct payload must precede STUN completion");
            release.send(()).unwrap();stun.await.unwrap();
            let mut reflexive=false;
            loop {match signals.next_signal().await.unwrap() {
                VideoIceSignal::Candidate {candidate,mid:actual}=>{assert_eq!(actual,mid);reflexive|=candidate.contains(" typ srflx");},
                VideoIceSignal::GatheringComplete=>break,
            }}
            assert!(reflexive);video.close();receiver.close().await.unwrap();
        }).await.unwrap();
    });
}

#[test]
fn trickle_cancelled_wait_and_close_do_not_lose_terminal_state() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5),async {
            let socket=tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let mut config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
            config.add_ice_server(vec![format!("stun:{}",socket.local_addr().unwrap())],String::new(),String::new()).unwrap();
            let video=WebRtcVideo::open_trickle(config).await.unwrap();let signals=video.signaling().unwrap();
            let count=video.events.signals.as_ref().unwrap().local_pending();
            for _ in 0..count {assert!(matches!(signals.next_signal().await.unwrap(),VideoIceSignal::Candidate {..}));}
            assert!(tokio::time::timeout(Duration::from_millis(10),signals.next_signal()).await.is_err());
            let waiting=signals.next_signal();tokio::pin!(waiting);
            std::future::poll_fn(|cx| {assert!(std::future::Future::poll(waiting.as_mut(),cx).is_pending());std::task::Poll::Ready(())}).await;
            video.close();assert!(matches!(waiting.await,Err(WebRtcError::Ended)));
            assert_eq!(signals.add_candidate("candidate:stale".into(),"0".into()),Err(WebRtcError::Ended));
        }).await.unwrap();
    });
}

#[test]
fn webrtc_loopback_preserves_video_payload_capture_clock_and_keyframe_feedback() {
    loopback(VideoFormat::H264 {profile_level_id: [66,224,31]});
}

#[test]
fn webrtc_hevc_loopback_preserves_both_idr_types_and_capture_clock() {
    let sps = [0,0,0,1,0x42,1,1,1,0x60,0,0,3,0,0xb0,0,0,3,0,0,3,0,0x7b];
    loopback(VideoFormat::HevcMain(crate::hevc_main_profile(&sps).unwrap()));
}

fn loopback(format: VideoFormat) {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(20),async {
            let mut video = WebRtcVideo::open(WebRtcConfig { ice_servers: Vec::new(),route_policy: RoutePolicy::Automatic,
                bind_addresses: vec!["127.0.0.1:0".into()],format }).await.unwrap();
            assert!(video.offer().contains("m=video") && video.offer().contains("a=sendonly"));
            assert!(!video.offer().contains("m=audio") && !video.offer().contains("m=application"));
            let mut media = MediaEngine::default();
            media.register_default_codecs().unwrap();
            let (sender,mut packets) = mpsc::channel(4);
            let receiver = PeerConnectionBuilder::new().with_media_engine(media)
                .with_handler(Arc::new(ReceiverEvents {packets: sender})).with_udp_addrs(vec!["127.0.0.1:0"])
                .build().await.unwrap();
            receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
            let answer = receiver.create_answer(None).await.unwrap();
            receiver.set_local_description(answer).await.unwrap();
            // Both candidates are explicit local UDP bindings. Their host
            // candidates are available synchronously, without public STUN.
            let answer = receiver.local_description().await.unwrap().sdp;
            video.answer(answer).await.unwrap();
            assert_eq!(video.next_update().await,VideoUpdate::Connected);
            assert!(video.take_keyframe_request());
            let feedback_not_before=video.last_keyframe_request.unwrap()+Duration::from_millis(250);
            assert!(video.offer().contains(format.mime().trim_start_matches("video/")));
            let (first,second) = match format {
                VideoFormat::H264 {..} => (vec![0x65,0x88,0x84,0x00,0x31],vec![0x41,0x9a,0x20,0x00,0x11]),
                VideoFormat::HevcMain(_) => (vec![0x26,1,0x31,0x20],vec![0x28,1,0x11,0x40]),
            };
            let unit = |payload: &[u8],pts,keyframe| {
                let mut annex_b = vec![0,0,0,1];annex_b.extend_from_slice(payload);
                EncodedFrame {codec: format.codec(),annex_b,keyframe,presentation_time_micros: pts}
            };
            video.send(unit(&first,1_000_000,true)).await.unwrap();
            let one = packets.recv().await.unwrap();
            assert_eq!(&one.payload[..],&first);
            assert!(one.header.marker);
            video.send(unit(&second,2_000_000,format.codec() == VideoCodec::Hevc)).await.unwrap();
            let two = packets.recv().await.unwrap();
            assert_eq!(&two.payload[..],&second);
            assert_eq!(two.header.timestamp.wrapping_sub(one.header.timestamp),90_000);
            assert_eq!(two.header.sequence_number,one.header.sequence_number.wrapping_add(1));
            assert_eq!(video.next_update().await,VideoUpdate::KeyframeRequested);
            assert!(Instant::now()>=feedback_not_before,"feedback must retain its 250 ms rate limit");
            let route = video.route_statistics().await;
            assert_eq!(route.route,MediaRoute::Direct,"selected candidate pair must prove the route");
            if route.bytes_sent == 0 {
                use webrtc::peer_connection::{RTCStatsReportEntry as Entry, StatsSelector};
                let report = video.peer.get_stats(Instant::now(),StatsSelector::None).await;
                // Structural diagnostics only: never print SDP, credentials,
                // candidate addresses or the full native report.
                for entry in report.iter() {
                    match entry {
                        Entry::Transport(value) => eprintln!("transport stats: selected reference present={} resolves={}",!value.selected_candidate_pair_id.is_empty(),report.get(&value.selected_candidate_pair_id).is_some()),
                        Entry::IceCandidatePair(value) => eprintln!("pair stats: nominated={} sent={} received={}",value.nominated,value.bytes_sent,value.bytes_received),
                        _ => {},
                    }
                }
            }
            assert!(route.bytes_sent > 0);
            let close = video.close_handle();
            let closing=tokio::spawn(async move {tokio::task::yield_now().await;close.close();});
            while video.next_update().await!=VideoUpdate::Ended {}
            closing.await.unwrap();
            assert!(video.ended() && !video.connected());
            receiver.close().await.unwrap();
        }).await.expect("bounded native WebRTC loopback");
    });
}
