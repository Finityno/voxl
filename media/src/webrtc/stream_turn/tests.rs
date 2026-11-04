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
fn webrtc_forced_relay_udp_uses_a_nominated_relay_and_carries_media() {forced_relay(RelayTransport::Udp);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_forced_relay_tcp_uses_a_nominated_relay_and_carries_media() {forced_relay(RelayTransport::Tcp);}

#[test]
#[ignore = "requires the isolated loopback coturn fixture and fresh test credentials"]
fn webrtc_forced_relay_tls_validates_certificate_and_carries_media() {forced_relay(RelayTransport::Tls);}

fn forced_relay(transport:RelayTransport) {
    use super::super::*;
    use webrtc::media_stream::track_remote::{TrackRemote,TrackRemoteEvent};
    use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
    struct Receiver {packets:mpsc::Sender<rtc::rtp::Packet>,gathered:Arc<AtomicBool>,wake:Arc<Notify>}
    #[async_trait]
    impl PeerConnectionEventHandler for Receiver {
        async fn on_ice_gathering_state_change(&self,state:RTCIceGatheringState) {
            if state==RTCIceGatheringState::Complete {self.gathered.store(true,Ordering::Release);self.wake.notify_one();}
        }
        async fn on_track(&self,track:Arc<dyn TrackRemote>) {
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
            let variable=match transport {RelayTransport::Udp=>"VOXL_TEST_TURN_UDP",RelayTransport::Tcp=>"VOXL_TEST_TURN_TCP",RelayTransport::Tls=>"VOXL_TEST_TURN_TLS",_=>unreachable!()};
            let url=std::env::var(variable).expect("fixture URL required");
            let parsed=rtc::ice::url::Url::parse_url(&url).unwrap();
            assert!(parsed.host.parse::<IpAddr>().unwrap().is_loopback(),"fixture must be loopback only");
            let server=RTCIceServer {urls:vec![url],username:std::env::var("VOXL_TEST_TURN_USERNAME").expect("fixture username"),credential:std::env::var("VOXL_TEST_TURN_PASSWORD").expect("fixture password")};
            let tls=if transport==RelayTransport::Tls {
                let der=std::fs::read(std::env::var("VOXL_TEST_TURN_CA_DER").expect("fixture CA required")).unwrap();
                let mut roots=rustls::RootCertStore::empty();roots.add(der.into()).unwrap();
                Some(Arc::new(ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider())).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth()))
            } else {None};
            let bindings=vec!["127.0.0.1:0".to_owned()];
            let mut video=WebRtcVideo::open_with_tls(WebRtcConfig {ice_servers:vec![server.clone()],route_policy:RoutePolicy::RelayOnly,bind_addresses:bindings.clone(),format:VideoFormat::H264 {profile_level_id:[66,224,31]}},tls.clone()).await.unwrap();
            let mut servers=vec![server];
            let receiver_relays=StreamRelays::prepare(&mut servers,&bindings,tls).await.unwrap();
            let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
            let (sender,mut packets)=mpsc::channel(4);
            let gathered=Arc::new(AtomicBool::new(false));let wake=Arc::new(Notify::new());
            let receiver=PeerConnectionBuilder::new().with_configuration(RTCConfigurationBuilder::new().with_ice_servers(servers).with_ice_transport_policy(RTCIceTransportPolicy::Relay).build())
                .with_media_engine(media).with_handler(Arc::new(Receiver {packets:sender,gathered:Arc::clone(&gathered),wake:Arc::clone(&wake)})).with_udp_addrs(bindings).build().await.unwrap();
            receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
            receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
            timeout(Duration::from_secs(6),async {while !gathered.load(Ordering::Acquire) {wake.notified().await;}}).await.unwrap();
            video.answer(receiver.local_description().await.unwrap().sdp).await.unwrap();
            assert_eq!(video.next_update().await,VideoUpdate::Connected);
            for (index,payload) in [vec![0x65,1,2,3,4],vec![0x41,5,6,7,8]].into_iter().enumerate() {
                let mut annex_b=vec![0,0,0,1];annex_b.extend_from_slice(&payload);
                video.send(EncodedFrame {codec:VideoCodec::H264,annex_b,keyframe:index==0,presentation_time_micros:1_000_000+(index as i64)*33_333}).await.unwrap();
                let received=packets.recv().await.unwrap();assert_eq!(&received.payload[..],&payload);
            }
            let route=video.route_statistics().await;
            assert_eq!(route.route,MediaRoute::Relayed,"host candidates cannot satisfy this fixture");
            assert_eq!(route.local_relay_transport,transport,"join nominated allocation to actual stream transport");
            assert!(route.bytes_sent>0);
            video.close();assert_eq!(video.next_update().await,VideoUpdate::Ended);
            receiver.close().await.unwrap();receiver_relays.close();
        }).await.expect("bounded forced-relay fixture");
    });
}
