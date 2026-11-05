//! Real synthetic encoder output over native UDP/DTLS/SRTP. Depacketization
//! verifies fragmentation/framing; it is not a browser/GPU decoder test.
use super::*;
use crate::av1::{Av1Frame,Av1MainProfile,av1_main_profile,fixture_frames};
use rtc::rtp::{codec::av1::Av1Depacketizer,packetizer::Depacketizer};

#[test]
fn av1_webrtc_fragmentation_preserves_obus_capture_clock_feedback_and_close() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(20),async {
            let mut frames=fixture_frames().into_iter();let first=frames.next().unwrap();let second=frames.next().unwrap();
            let profile=av1_main_profile(&first.temporal_unit).unwrap();let format=VideoFormat::Av1Main(profile);
            let mut video=WebRtcVideo::open(WebRtcConfig::direct_first(format,vec!["127.0.0.1:0".into()])).await.unwrap();
            assert!(video.offer().contains("AV1/90000"));assert!(video.offer().contains("profile=0;level-idx=0;tier=0"));
            assert!(!video.offer().contains("H264/90000") && !video.offer().contains("H265/90000"));
            let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
            let (sender,mut packets)=mpsc::channel(16);
            let receiver=PeerConnectionBuilder::new().with_media_engine(media).with_handler(Arc::new(ReceiverEvents {packets:sender})).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap();
            receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
            let answer=receiver.create_answer(None).await.unwrap();receiver.set_local_description(answer).await.unwrap();
            video.answer(receiver.local_description().await.unwrap().sdp).await.unwrap();
            assert_eq!(video.next_update().await,VideoUpdate::Connected);assert!(video.take_keyframe_request());
            let invalid=||Av1Frame {temporal_unit:vec![0x32,0xff],keyframe:true,presentation_time_micros:0};
            assert_eq!(video.send_av1_with_stamp(invalid()).await,Err(WebRtcError::InvalidFrame));
            assert_eq!(video.send_av1(Av1Frame {temporal_unit:second.temporal_unit.clone(),keyframe:false,presentation_time_micros:0}).await,Err(WebRtcError::InvalidFrame));
            assert!(video.first_pts.is_none() && video.last_pts.is_none());
            video.timestamp=u32::MAX-4500;
            let expected=first.temporal_unit[2..].to_vec();let stamp=video.send_av1_with_stamp(first).await.unwrap();
            let mut depacketizer=Av1Depacketizer::new();let mut restored=Vec::new();let mut sequence:Option<u16>=None;let mut packet_count=0;
            loop {
                let packet=packets.recv().await.unwrap();packet_count+=1;
                assert!(packet.payload.len()<=1100);assert_eq!(packet.header.timestamp,stamp.rtp_timestamp);
                if let Some(previous)=sequence {assert_eq!(packet.header.sequence_number,previous.wrapping_add(1));}
                sequence=Some(packet.header.sequence_number);
                assert_eq!(packet.payload[0]&8!=0,packet_count==1,"N marks only the beginning of the coded sequence");
                restored.extend_from_slice(&depacketizer.depacketize(&packet.payload).unwrap());
                if packet.header.marker {assert!(!depacketizer.y);break;}
            }
            assert!(packet_count>1,"fixture must exercise fragmentation");assert_eq!(restored,expected);assert_eq!(stamp.generation,0);
            let expected=second.temporal_unit[2..].to_vec();let second_stamp=video.send_av1_with_stamp(second).await.unwrap();
            let packet=packets.recv().await.unwrap();assert!(packet.header.marker);assert_eq!(packet.payload[0]&8,0);
            assert_eq!(packet.header.sequence_number,sequence.unwrap().wrapping_add(1));
            assert_eq!(second_stamp.rtp_timestamp.wrapping_sub(stamp.rtp_timestamp),9000);
            assert_eq!(&depacketizer.depacketize(&packet.payload).unwrap()[..],&expected);
            assert_eq!(video.next_update().await,VideoUpdate::KeyframeRequested);
            let route=video.route_statistics().await;assert_eq!(route.route,MediaRoute::Direct);assert!(route.bytes_sent>0);
            video.close();assert_eq!(video.next_update().await,VideoUpdate::Ended);
            assert_eq!(video.send_av1(invalid()).await,Err(WebRtcError::Ended));assert!(!video.connected());
            receiver.close().await.unwrap();
        }).await.expect("bounded AV1 transport loopback");
    });
}

#[test]
fn av1_webrtc_rejects_an_answer_below_the_actual_encoded_level() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(15),async {
            let format=VideoFormat::Av1Main(Av1MainProfile {level_idx:8,high_tier:false});
            let mut video=WebRtcVideo::open(WebRtcConfig::direct_first(format,vec!["127.0.0.1:0".into()])).await.unwrap();
            let mut media=MediaEngine::default();
            media.register_codec(RTCRtpCodecParameters {rtp_codec:RTCRtpCodec {mime_type:"video/AV1".into(),clock_rate:90_000,sdp_fmtp_line:format.fmtp(),..Default::default()},payload_type:102},RtpCodecKind::Video).unwrap();
            let receiver=PeerConnectionBuilder::new().with_media_engine(media).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap();
            receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
            receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
            let answer=receiver.local_description().await.unwrap().sdp;assert!(answer.contains("level-idx=8"));
            let answer=answer.replace("level-idx=8","level-idx=5");
            assert_eq!(video.answer(answer).await,Err(WebRtcError::Configuration));
            assert!(video.ended() && !video.connected());receiver.close().await.unwrap();
        }).await.expect("bounded AV1 answer refusal");
    });
}
