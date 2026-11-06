use super::*;

async fn pair(feedback:bool)->(WebRtcVideo,Arc<dyn PeerConnection>,mpsc::Receiver<rtc::rtp::Packet>) {
    let config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
    let mut video=WebRtcVideo::open_controlled(config,SendControl::new(250,1000,1000).unwrap()).await.unwrap();
    assert!(video.offer().contains("transport-cc") && video.offer().contains("transport-wide-cc"));
    let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
    let registry=if feedback {register_default_interceptors(Registry::new(),&mut media).unwrap()} else {Registry::new()};
    let (sender,packets)=mpsc::channel(128);
    let receiver:Arc<dyn PeerConnection>=Arc::new(PeerConnectionBuilder::new().with_media_engine(media).with_interceptor_registry(registry)
        .with_handler(Arc::new(ReceiverEvents {packets:sender})).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap());
    receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
    receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
    video.answer(receiver.local_description().await.unwrap().sdp).await.unwrap();
    assert_eq!(video.next_update().await,VideoUpdate::Connected);
    (video,receiver,packets)
}
fn frame(keyframe:bool,pts:i64)->EncodedFrame {
    let mut annex_b=vec![0,0,0,1,if keyframe {0x65} else {0x41}];annex_b.extend(vec![0x77;12_000]);
    EncodedFrame {codec:VideoCodec::H264,annex_b,keyframe,presentation_time_micros:pts}
}
async fn receive(packets:&mut mpsc::Receiver<rtc::rtp::Packet>)->(u32,Vec<u8>) {
    let mut data=Vec::new();let mut last:Option<(u16,u32)>=None;
    loop {
        let packet=packets.recv().await.unwrap();
        if let Some((sequence,timestamp))=last {assert_eq!(packet.header.sequence_number,sequence.wrapping_add(1));assert_eq!(packet.header.timestamp,timestamp);}
        if packet.payload[0]&31==28 {
            if packet.payload[1]&0x80!=0 {data.push((packet.payload[0]&0xe0)|(packet.payload[1]&31));}
            data.extend_from_slice(&packet.payload[2..]);
        } else {data.extend_from_slice(&packet.payload);}
        last=Some((packet.header.sequence_number,packet.header.timestamp));
        if packet.header.marker {return (packet.header.timestamp,data);}
    }
}
#[test]
fn adaptive_udp_loopback_preserves_access_units_clock_and_real_twcc_feedback() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {tokio::time::timeout(Duration::from_secs(15),async {
        let (mut video,receiver,mut packets)=pair(true).await;
        assert!(video.bitrate_estimate().is_none());
        let first=frame(true,1_000_000);let expected=first.annex_b[4..].to_vec();let started=Instant::now();
        let stamp=video.send_with_stamp(first).await.unwrap();
        assert!(started.elapsed()>=Duration::from_millis(20),"a frame larger than the burst must wait for paced release");
        let (timestamp,data)=receive(&mut packets).await;assert_eq!(data,expected);assert_eq!(timestamp,stamp.rtp_timestamp);
        let second=frame(false,2_000_000);let expected=second.annex_b[4..].to_vec();
        video.send(second).await.unwrap();let (next,data)=receive(&mut packets).await;assert_eq!(data,expected);assert_eq!(next.wrapping_sub(timestamp),90_000);
        tokio::time::timeout(Duration::from_secs(3),async {
            loop {match video.next_update().await {VideoUpdate::BitrateTarget(kbps)=>{assert!((250..=850).contains(&kbps));break;},VideoUpdate::Ended=>panic!("controlled transport ended"),_=>{}}}
        }).await.unwrap();
        let estimate=video.bitrate_estimate().unwrap();assert!((250..=1000).contains(&estimate.target_kbps));assert!(estimate.age<Duration::from_secs(2));
        assert_eq!(video.route_statistics().await.route,MediaRoute::Direct);
        video.close();assert!(video.ended());receiver.close().await.unwrap();
    }).await.expect("bounded controlled UDP loopback");});
}
#[test]
fn adaptive_cancelled_partial_frame_closes_before_queued_rtp_can_escape() {
    use std::future::Future;
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {tokio::time::timeout(Duration::from_secs(10),async {
        let (mut video,receiver,mut packets)=pair(true).await;
        let mut pending=Box::pin(video.send(frame(true,1_000_000)));
        std::future::poll_fn(|cx| {assert!(pending.as_mut().poll(cx).is_pending());std::task::Poll::Ready(())}).await;
        drop(pending);assert!(video.ended());
        assert!(!matches!(tokio::time::timeout(Duration::from_millis(200),packets.recv()).await,Ok(Some(_))));
        receiver.close().await.unwrap();
    }).await.expect("bounded cancelled controlled send");});
}
#[test]
fn adaptive_receiver_without_twcc_keeps_estimates_unknown_and_delivers_paced_media() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {tokio::time::timeout(Duration::from_secs(10),async {
        let (mut video,receiver,mut packets)=pair(false).await;
        let unit=frame(true,1_000_000);let expected=unit.annex_b[4..].to_vec();
        video.send(unit).await.unwrap();assert_eq!(receive(&mut packets).await.1,expected);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(video.connected() && video.bitrate_estimate().is_none());
        video.close();receiver.close().await.unwrap();
    }).await.expect("bounded no-feedback controlled send");});
}
