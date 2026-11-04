//! Real UDP/DTLS/SRTP after late ICE configuration. No capture, input or decoder.
use super::*;
use std::net::IpAddr;
use tokio::{net::UdpSocket,time::timeout};

#[test]
fn late_relay_configuration_restarts_ice_and_fences_old_signaling() {late_configuration(None);}

#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_udp_configuration_carries_media() {late_configuration(Some(RelayTransport::Udp));}
#[test]
#[ignore = "requires isolated coturn and a namespace blocking direct host UDP"]
fn late_relay_tcp_configuration_carries_media() {late_configuration(Some(RelayTransport::Tcp));}
#[test]
#[ignore = "requires isolated coturn TLS and a namespace blocking direct host UDP"]
fn late_relay_tls_configuration_carries_media() {late_configuration(Some(RelayTransport::Tls));}

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

fn late_configuration(relay:Option<RelayTransport>) {
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
        let mut video=WebRtcVideo::open_mode(WebRtcConfig::direct_first(format,bindings.clone()),tls,true).await.unwrap();
        let initial_offer=video.offer().to_owned();let old_signals=video.signaling().unwrap();
        let mut media=MediaEngine::default();media.register_default_codecs().unwrap();let (packets,mut received)=mpsc::channel(4);
        let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());
        let receiver=Arc::new(PeerConnectionBuilder::new().with_media_engine(media)
            .with_handler(Arc::new(TrickleReceiver {receiver:ReceiverEvents {packets},gathered:Arc::clone(&gathered),wake:Arc::clone(&wake)}))
            .with_udp_addrs(bindings.clone()).build().await.unwrap());
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
        let offer=video.begin_relay_fallback(fallback).await.unwrap();
        eprintln!("late_fallback_stage=restart_offer");
        assert_eq!(offer.generation,1);
        assert!(field(&initial_offer,"a=ice-ufrag:")!=field(&offer.sdp,"a=ice-ufrag:"),"restart must advertise fresh ICE credentials");
        assert!(field(&initial_offer,"a=fingerprint:")==field(&offer.sdp,"a=fingerprint:"),"DTLS identity must stay pinned");
        assert_eq!(old_signals.add_candidate("candidate:old".into(),"0".into()),Err(WebRtcError::Ended));
        assert!(matches!(old_signals.next_signal().await,Err(WebRtcError::Ended)));
        assert_eq!(video.answer(old_answer).await,Err(WebRtcError::Configuration));assert!(!video.ended());
        let signals=video.signaling().unwrap();assert_eq!(signals.generation(),1);
        gathered.store(false,Ordering::Release);
        receiver.set_remote_description(RTCSessionDescription::offer(strip_candidates(&offer.sdp)).unwrap()).await.unwrap();
        eprintln!("late_fallback_stage=remote_offer");
        let outgoing=signals.clone();let peer=Arc::clone(&receiver);
        let candidates=tokio::spawn(async move {
            let mut discovered=false;
            loop {match outgoing.next_signal().await.unwrap() {
                VideoIceSignal::Candidate {candidate,mid}=>{
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
        if let Some(stun)=stun {stun.await.unwrap();}
        video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,9,8,7,6],keyframe:true,presentation_time_micros:1_000_000}).await.unwrap();
        assert_eq!(&received.recv().await.unwrap().payload[..],&[0x65,9,8,7,6]);
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
