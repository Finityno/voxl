//! Exercise the real UDP/DTLS/SRTP path without a display, OS input or decoder.
//! Payloads are synthetic NAL units: this is transport, not H.264 quality proof.
use super::*;
use webrtc::media_stream::track_remote::{TrackRemote,TrackRemoteEvent};
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use tokio::sync::mpsc;

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
            while !video.connected() {
                assert!(!video.ended(),"peer ended before connection");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(video.take_keyframe_request());
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
            while !video.take_keyframe_request() { tokio::time::sleep(Duration::from_millis(10)).await; }
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
            close.close();
            assert!(video.ended() && !video.connected());
            receiver.close().await.unwrap();
        }).await.expect("bounded native WebRTC loopback");
    });
}
