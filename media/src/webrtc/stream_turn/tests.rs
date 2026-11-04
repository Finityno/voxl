use super::*;

#[test]
fn turn_stream_framing_preserves_coalesced_and_fragmented_packets() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        timeout(Duration::from_secs(2),async {
            let stun=vec![0,1,0,0,0x21,0x12,0xa4,0x42,1,2,3,4,5,6,7,8,9,10,11,12];
            let channel=vec![0x40,1,0,3,7,8,9];
            let (mut writer,mut reader)=tokio::io::duplex(8);
            let expected_stun=stun.clone();let expected_channel=channel.clone();
            let writing=async {
                let mut wire=stun;wire.extend_from_slice(&channel);wire.push(0);
                for byte in wire {writer.write_all(&[byte]).await.unwrap();tokio::task::yield_now().await;}
            };
            let reading=async {
                assert_eq!(read_frame(&mut reader).await.unwrap(),expected_stun);
                assert_eq!(read_frame(&mut reader).await.unwrap(),expected_channel);
            };
            tokio::join!(writing,reading);
        }).await.unwrap();
    });
}

#[test]
fn turn_stream_datagrams_reject_invalid_lengths_and_framing() {
    let mut maximum=vec![0;MAX_DATAGRAM];
    maximum[..4].copy_from_slice(&[0x40,0,((MAX_DATAGRAM-4)>>8) as u8,(MAX_DATAGRAM-4) as u8]);
    assert_eq!(datagram_length(&maximum).unwrap(),MAX_DATAGRAM);
    maximum.push(0);assert!(datagram_length(&maximum).is_err());
    assert_eq!(datagram_length(&[0x40,0,0,1,7]).unwrap(),5);
    assert_eq!(datagram_length(&[0x40,0,0,1,7,0,0,0]).unwrap(),5);
    for data in [&[0x40,0,0,2,7][..],&[0x80,0,0,0],&[0x40,0,0,0,0],&[0,0,0,0,0,0,0,0]] {
        assert!(datagram_length(data).is_err());
    }
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        for header in [[0x40,0,0xff,0xff],[0,0,0,1],[0x80,0,0,0]] {
            let mut bytes=&header[..];assert!(read_frame(&mut bytes).await.is_err());
        }
    });
}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_forced_relay_udp_uses_a_nominated_relay_and_carries_media() {relay_connection(RelayTransport::Udp,super::super::RoutePolicy::RelayOnly,true,GatherMode::Full);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_forced_relay_tcp_uses_a_nominated_relay_and_carries_media() {relay_connection(RelayTransport::Tcp,super::super::RoutePolicy::RelayOnly,true,GatherMode::Full);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_forced_relay_tls_validates_certificate_and_carries_media() {relay_connection(RelayTransport::Tls,super::super::RoutePolicy::RelayOnly,true,GatherMode::Full);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_direct_media_survives_an_unused_tcp_relay_failure() {relay_connection(RelayTransport::Tcp,super::super::RoutePolicy::Automatic,true,GatherMode::Full);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_default_sdk_relay_allocates_per_interface_for_comparison() {relay_connection(RelayTransport::Udp,super::super::RoutePolicy::RelayOnly,false,GatherMode::Full);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn trickle_forced_relay_udp_carries_media_without_candidates_in_sdp() {relay_connection(RelayTransport::Udp,super::super::RoutePolicy::RelayOnly,true,GatherMode::Trickle);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn trickle_forced_relay_tcp_carries_media_without_candidates_in_sdp() {relay_connection(RelayTransport::Tcp,super::super::RoutePolicy::RelayOnly,true,GatherMode::Trickle);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn trickle_forced_relay_tls_carries_media_without_candidates_in_sdp() {relay_connection(RelayTransport::Tls,super::super::RoutePolicy::RelayOnly,true,GatherMode::Trickle);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn lazy_turn_immediate_native_baseline_retains_allocation() {relay_connection(RelayTransport::Udp,super::super::RoutePolicy::Automatic,true,GatherMode::TrickleImmediate);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn lazy_turn_direct_media_skips_the_native_relay_allocation() {relay_connection(RelayTransport::Udp,super::super::RoutePolicy::Automatic,true,GatherMode::Trickle);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn lazy_turn_udp_fallback_connects_when_direct_candidates_are_unusable() {relay_connection(RelayTransport::Udp,super::super::RoutePolicy::Automatic,true,GatherMode::DeferredFallback);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn lazy_turn_tcp_fallback_connects_when_direct_candidates_are_unusable() {relay_connection(RelayTransport::Tcp,super::super::RoutePolicy::Automatic,true,GatherMode::DeferredFallback);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn lazy_turn_tls_fallback_connects_when_direct_candidates_are_unusable() {relay_connection(RelayTransport::Tls,super::super::RoutePolicy::Automatic,true,GatherMode::DeferredFallback);}

#[test]
#[ignore = "requires the isolated loopback coturn TLS fixture"]
fn webrtc_turn_tls_rejects_a_certificate_outside_the_trust_store() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let url=rtc::ice::url::Url::parse_url(&std::env::var("VOXL_TEST_TURN_TLS").expect("fixture TLS URL")).unwrap();
        assert!(url.host.parse::<IpAddr>().unwrap().is_loopback());
        let socket=Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let source=socket.local_addr().unwrap();
        let (sender,receiver)=mpsc::channel(1);
        sender.try_send(vec![0,1,0,0,0x21,0x12,0xa4,0x42,1,2,3,4,5,6,7,8,9,10,11,12]).unwrap();
        let state=AllocationState {allocations:Arc::default(),failures:Arc::default(),changes:Arc::default(),address:None};
        let target=Arc::new(Target {host:url.host,port:url.port,transport:RelayTransport::Tls,tls:Some(default_tls().unwrap())});
        let error=timeout(Duration::from_secs(5),session(socket,source,target,receiver,state)).await.unwrap().unwrap_err();
        assert!(matches!(error.get_ref().and_then(|value|value.downcast_ref::<rustls::Error>()),Some(rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer))));
    });
}

#[derive(Clone,Copy,PartialEq,Eq)]
enum GatherMode {Full,Trickle,DeferredFallback,TrickleImmediate}

fn relay_connection(expected_transport:RelayTransport,policy:super::super::RoutePolicy,limit_receiver_interfaces:bool,mode:GatherMode) {
    use super::super::*;
    use std::sync::atomic::AtomicUsize;
    use webrtc::media_stream::track_remote::{TrackRemote,TrackRemoteEvent};
    use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
    let trickle=mode!=GatherMode::Full;
    let blocked_direct=mode==GatherMode::DeferredFallback;
    struct CandidateGate {open:bool,pending:Vec<String>}
    struct Receiver {gate:Arc<Mutex<CandidateGate>>,incremental:Option<(VideoSignaling,String,Arc<AtomicUsize>)>,packets:mpsc::Sender<rtc::rtp::Packet>,gathered:Arc<AtomicBool>,connected:Arc<AtomicBool>,track_seen:Arc<AtomicBool>,wake:Arc<Notify>}
    #[async_trait]
    impl PeerConnectionEventHandler for Receiver {
        async fn on_ice_candidate(&self,event:RTCPeerConnectionIceEvent) {
            if let Some((signals,mid,count))=&self.incremental {
                let candidate=event.candidate.to_json().unwrap();
                if !candidate.candidate.is_empty() {
                    let relay=candidate.candidate.contains(" typ relay");
                    let mut gate=self.gate.lock().unwrap();
                    if gate.open || !relay {
                        signals.add_candidate(candidate.candidate,mid.clone()).unwrap();
                        count.fetch_add(1,Ordering::AcqRel);
                    } else {
                        assert!(gate.pending.len()<64);gate.pending.push(candidate.candidate);
                    }
                }
            }
        }
        async fn on_ice_gathering_state_change(&self,state:RTCIceGatheringState) {
            if state==RTCIceGatheringState::Complete {self.gathered.store(true,Ordering::Release);self.wake.notify_one();}
        }
        async fn on_connection_state_change(&self,state:RTCPeerConnectionState) {
            self.connected.store(state==RTCPeerConnectionState::Connected,Ordering::Release);self.wake.notify_one();
        }
        async fn on_track(&self,track:Arc<dyn TrackRemote>) {
            self.track_seen.store(true,Ordering::Release);
            let packets=self.packets.clone();
            tokio::spawn(async move {
                while let Some(event)=track.poll().await {
                    if let TrackRemoteEvent::OnRtpPacket(packet)=event {
                        let ssrc=packet.header.ssrc;
                        if packets.try_send(packet).is_err() {break;}
                        let _=track.write_rtcp(vec![Box::new(PictureLossIndication {sender_ssrc:7,media_ssrc:ssrc})]).await;
                    }
                }
            });
        }
    }
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        timeout(Duration::from_secs(25),async {
            let variable=match expected_transport {RelayTransport::Udp=>"VOXL_TEST_TURN_UDP",RelayTransport::Tcp=>"VOXL_TEST_TURN_TCP",RelayTransport::Tls=>"VOXL_TEST_TURN_TLS",_=>unreachable!()};
            let url=std::env::var(variable).expect("fixture URL required");
            let parsed=rtc::ice::url::Url::parse_url(&url).unwrap();
            assert!(parsed.host.parse::<IpAddr>().unwrap().is_loopback(),"fixture must be loopback only");
            let server=RTCIceServer {urls:vec![url],username:std::env::var("VOXL_TEST_TURN_USERNAME").expect("fixture username"),credential:std::env::var("VOXL_TEST_TURN_PASSWORD").expect("fixture password")};
            let tls=if expected_transport==RelayTransport::Tls {
                let der=std::fs::read(std::env::var("VOXL_TEST_TURN_CA_DER").expect("fixture CA required")).unwrap();
                let mut roots=rustls::RootCertStore::empty();roots.add(der.into()).unwrap();
                Some(Arc::new(ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider())).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth()))
            } else {None};
            // Two interfaces must remain available for direct ICE, without
            // multiplying TURN allocations for this one reachable IPv4 route.
            let bindings=if blocked_direct {
                assert_eq!(std::env::var("VOXL_TEST_BLOCK_DIRECT_UDP").as_deref(),Ok("1"),"delayed fallback requires the private network namespace fixture");
                // Removing host candidates does not prevent peer-reflexive
                // discovery from finding a direct path. Prove actual packet
                // loss between every host-interface pair before testing TURN.
                for source in ["127.0.0.2","127.0.0.3"] {
                    for target in ["127.0.0.2","127.0.0.3"] {
                        let sender=UdpSocket::bind(format!("{source}:0")).await.unwrap();
                        let receiver=UdpSocket::bind(format!("{target}:0")).await.unwrap();
                        sender.send_to(b"direct path probe",receiver.local_addr().unwrap()).await.unwrap();
                        let mut bytes=[0;64];
                        assert!(timeout(Duration::from_millis(20),receiver.recv_from(&mut bytes)).await.is_err(),"fixture must block all direct host UDP paths");
                    }
                }
                vec!["127.0.0.2:0".to_owned(),"127.0.0.3:0".to_owned()]
            } else {vec!["127.0.0.1:0".to_owned(),"127.0.0.2:0".to_owned()]};
            let retained_bindings=bindings.clone();
            let opened_at=Instant::now();
            let config=WebRtcConfig {ice_servers:vec![server.clone()],route_policy:policy,bind_addresses:bindings.clone(),format:VideoFormat::H264 {profile_level_id:[66,224,31]}};
            let mut video=if mode==GatherMode::TrickleImmediate {WebRtcVideo::open_with_grace(config,tls.clone(),true,Duration::ZERO).await}
                else {WebRtcVideo::open_mode(config,tls.clone(),trickle).await}.unwrap();
            let relay_count=|sdp:&str| {
                let rows:Vec<_>=sdp.lines().filter(|line|line.starts_with("a=candidate:") && line.contains(" typ relay")).map(|line|line.split_whitespace().collect::<Vec<_>>()).collect();
                assert!(rows.iter().all(|row|row.len()>=8),"relay candidate must include its transport address");
                let endpoints:std::collections::HashSet<_>=rows.iter().map(|row|(row[2],row[4],row[5])).collect();
                let components:Vec<_>=rows.iter().map(|row|row[1].parse::<u16>().unwrap()).collect();
                // An allocation is a relay transport/address/port, not an SDP
                // row. Components or repeated SDP placement can name it again.
                // Never print candidate addresses or session authentication.
                eprintln!("relay gather: rows={} unique_transport_endpoints={} components={components:?}",rows.len(),endpoints.len());
                endpoints.len()
            };
            if !trickle {assert_eq!(relay_count(video.offer()),1,"sender must use one authenticated relay allocation for both local IPv4 bindings");}
            let mut servers=vec![server];
            let receiver_relays=StreamRelays::prepare(&mut servers,&bindings,tls,Arc::new(ChangeSignal::default()),Duration::ZERO).await.unwrap();
            let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
            let (sender,mut packets)=mpsc::channel(4);
            let gathered=Arc::new(AtomicBool::new(false));let connected=Arc::new(AtomicBool::new(false));let track_seen=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());
            let receiver=PeerConnectionBuilder::new();
            // Keep an independent comparison against the SDK's default None
            // preference. Only the receiver changes; the sender stays limited.
            let receiver=if limit_receiver_interfaces {receiver.with_runtime(receiver_relays.runtime())} else {receiver};
            let expected_receiver_allocations=if limit_receiver_interfaces {1} else {bindings.len()};
            let remote_candidates=Arc::new(AtomicUsize::new(0));
            // Exchange valid host candidates even when their packets are
            // blackholed. Withhold only the receiver relay: that alone could
            // connect before the native allocation deadline we must exercise.
            let gate=Arc::new(Mutex::new(CandidateGate {open:!blocked_direct,pending:Vec::new()}));
            let signals=video.signaling();
            let incremental=signals.clone().map(|signals|(signals,video.events.media_mid.get().unwrap().clone(),Arc::clone(&remote_candidates)));
            let receiver=Arc::new(receiver.with_configuration(RTCConfigurationBuilder::new().with_ice_servers(servers).with_ice_transport_policy(if policy==RoutePolicy::RelayOnly {RTCIceTransportPolicy::Relay} else {RTCIceTransportPolicy::All}).build())
                .with_media_engine(media).with_handler(Arc::new(Receiver {gate:Arc::clone(&gate),incremental,packets:sender,gathered:Arc::clone(&gathered),connected:Arc::clone(&connected),track_seen:Arc::clone(&track_seen),wake:Arc::clone(&wake)})).with_udp_addrs(bindings).build().await.unwrap());
            let without_candidates=|sdp:&str|sdp.lines().filter(|line|!line.starts_with("a=candidate:") && *line!="a=end-of-candidates").collect::<Vec<_>>().join("\r\n")+"\r\n";
            let offer=if trickle {without_candidates(video.offer())} else {video.offer().to_owned()};
            receiver.set_remote_description(RTCSessionDescription::offer(offer).unwrap()).await.unwrap();
            let candidate_task=signals.map(|signals| {
                let target=Arc::clone(&receiver);let gate=Arc::clone(&gate);let remote_candidates=Arc::clone(&remote_candidates);
                tokio::spawn(async move {
                    let mut relays=0;
                    loop {match signals.next_signal().await.unwrap() {
                        VideoIceSignal::Candidate {candidate,mid}=>{
                            let relay=candidate.contains(" typ relay");
                            if relay {
                                relays+=1;
                                if blocked_direct {
                                    assert!(opened_at.elapsed()>=Duration::from_millis(250),"automatic relay cannot allocate before its grace");
                                    let mut gate=gate.lock().unwrap();gate.open=true;
                                    for candidate in std::mem::take(&mut gate.pending) {
                                        signals.add_candidate(candidate,mid.clone()).unwrap();remote_candidates.fetch_add(1,Ordering::AcqRel);
                                    }
                                }
                            }
                            target.add_ice_candidate(super::super::signaling::candidate(candidate,mid)).await.unwrap();
                        },
                        VideoIceSignal::GatheringComplete=>break,
                    }}
                    relays
                })
            });
            receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
            if !trickle {timeout(Duration::from_secs(6),async {while !gathered.load(Ordering::Acquire) {wake.notified().await;}}).await.unwrap();}
            let answer=receiver.local_description().await.unwrap().sdp;
            let answer=if trickle {without_candidates(&answer)} else {answer};
            video.answer(answer).await.unwrap();
            assert_eq!(timeout(Duration::from_secs(5),video.next_update()).await.expect("forced-relay ICE/DTLS connection deadline"),VideoUpdate::Connected);
            timeout(Duration::from_secs(5),async {while !connected.load(Ordering::Acquire) {wake.notified().await;}}).await.expect("receiver ICE/DTLS connection deadline");
            assert!(video.take_keyframe_request());
            let skip_native_turn=mode==GatherMode::Trickle && policy==RoutePolicy::Automatic;
            let expected_sender_allocations=if skip_native_turn {0} else {1};
            if let Some(task)=candidate_task {
                let relays=timeout(Duration::from_secs(6),task).await.unwrap().unwrap();
                if skip_native_turn {assert_eq!(relays,0,"a connected direct peer must skip deferred TURN");}
                else {assert!(relays>0,"host relay candidate must cross the incremental path");}
                assert!(remote_candidates.load(Ordering::Acquire)>0,"viewer candidates must cross the incremental path");
            }
            timeout(Duration::from_secs(6),async {while !gathered.load(Ordering::Acquire) {wake.notified().await;}}).await.unwrap();
            let local=video.peer.local_description().await.unwrap().sdp;
            assert_eq!(relay_count(&local),expected_sender_allocations,"sender must follow its deferred allocation policy");
            if policy==RoutePolicy::Automatic {
                for address in &retained_bindings {
                    let ip=address.parse::<SocketAddr>().unwrap().ip().to_string();
                    assert!(local.lines().any(|line|line.contains(&ip) && line.contains(" typ host")),"both direct interfaces must survive relay optimization");
                }
            }
            assert_eq!(relay_count(&receiver.local_description().await.unwrap().sdp),expected_receiver_allocations,"receiver allocation count must match its explicit policy");
            eprintln!("relay fanout: trickle={trickle} sender_bindings=2 receiver_bindings=2 sender_allocations={expected_sender_allocations} receiver_allocations={expected_receiver_allocations}");
            for (index,payload) in [vec![0x65,1,2,3,4],vec![0x41,5,6,7,8]].into_iter().enumerate() {
                let mut annex_b=vec![0,0,0,1];annex_b.extend_from_slice(&payload);
                video.send(EncodedFrame {codec:VideoCodec::H264,annex_b,keyframe:index==0,presentation_time_micros:1_000_000+(index as i64)*33_333}).await.unwrap();
                let received=match timeout(Duration::from_secs(5),packets.recv()).await {
                    Ok(Some(packet))=>packet,
                    _=>{
                        eprintln!("media deadline: frame={index} receiver connected={} track seen={} sender {:?}",connected.load(Ordering::Acquire),track_seen.load(Ordering::Acquire),video.route_statistics().await);
                        use webrtc::peer_connection::{RTCStatsReportEntry as Entry,StatsSelector};
                        for entry in receiver.get_stats(Instant::now(),StatsSelector::None).await.iter() {
                            match entry {
                                Entry::IceCandidatePair(pair)=>eprintln!("receiver pair: nominated={} sent={} received={}",pair.nominated,pair.bytes_sent,pair.bytes_received),
                                Entry::InboundRtp(stream)=>eprintln!("receiver RTP: bytes={}",stream.bytes_received),
                                _=>{},
                            }
                        }
                        panic!("forced-relay media receive deadline");
                    }
                };
                assert_eq!(&received.payload[..],&payload);
            }
            let route=video.route_statistics().await;
            let relay_only=policy==RoutePolicy::RelayOnly || blocked_direct;
            assert_eq!(route.route,if relay_only {MediaRoute::Relayed} else {MediaRoute::Direct},"prove the selected route under the requested policy");
            assert_eq!(route.local_relay_transport,if relay_only {expected_transport} else {RelayTransport::Unknown},"join nominated allocation to actual stream transport");
            assert!(route.bytes_sent>0);
            if !relay_only {
                video.closer.relays.close();
                assert_eq!(timeout(Duration::from_secs(2),video.next_update()).await.unwrap(),VideoUpdate::KeyframeRequested,"unused relay failure must leave direct video healthy");
                video.send(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65,9,8,7,6],keyframe:true,presentation_time_micros:2_000_000}).await.unwrap();
                assert_eq!(&timeout(Duration::from_secs(2),packets.recv()).await.unwrap().unwrap().payload[..],&[0x65,9,8,7,6]);
                assert_eq!(video.route_statistics().await.route,MediaRoute::Direct);
                video.close();
            }
            else if expected_transport==RelayTransport::Udp {video.close();}
            else {
                // Lose the allocated stream without closing the peer first.
                // Its media update must wake and end the selected failed path.
                assert!(!video.closer.closed.load(Ordering::Acquire));
                video.closer.relays.close();
            }
            while video.next_update().await!=VideoUpdate::Ended {}
            receiver.close().await.unwrap();receiver_relays.close();
        }).await.expect("bounded forced-relay fixture");
    });
}
