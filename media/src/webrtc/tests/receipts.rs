//! Actual UDP/DTLS/SCTP and RTP, with synthetic NALs and peer claims. These
//! tests exercise transport binding, not native decoding or picture quality.
use super::*;
use webrtc::data_channel::{DataChannel,DataChannelEvent};

struct Receiver {media:ReceiverEvents,channels:mpsc::Sender<Arc<dyn DataChannel>>}
#[async_trait]
impl PeerConnectionEventHandler for Receiver {
    async fn on_track(&self,track:Arc<dyn TrackRemote>) {self.media.on_track(track).await;}
    async fn on_data_channel(&self,channel:Arc<dyn DataChannel>) {self.channels.try_send(channel).unwrap();}
}
async fn pair()->(WebRtcVideo,Arc<dyn PeerConnection>,Arc<dyn DataChannel>,mpsc::Receiver<rtc::rtp::Packet>) {
    let config=WebRtcConfig::direct_first(VideoFormat::H264 {profile_level_id:[66,224,31]},vec!["127.0.0.1:0".into()]);
    let video=WebRtcVideo::open_controlled(config,SendControl::new(250,1000,1000).unwrap().with_frame_receipts()).await.unwrap();
    assert!(video.offer().contains("m=application"));assert!(!video.frame_receipts_available());
    let mut media=MediaEngine::default();media.register_default_codecs().unwrap();
    let (sender,packets)=mpsc::channel(4);let (channels,mut incoming)=mpsc::channel(1);
    let receiver:Arc<dyn PeerConnection>=Arc::new(PeerConnectionBuilder::new().with_media_engine(media).with_handler(Arc::new(Receiver {media:ReceiverEvents {packets:sender},channels}))
        .with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap());
    receiver.set_remote_description(RTCSessionDescription::offer(video.offer().to_owned()).unwrap()).await.unwrap();
    receiver.set_local_description(receiver.create_answer(None).await.unwrap()).await.unwrap();
    let mut video=video;video.answer(receiver.local_description().await.unwrap().sdp).await.unwrap();
    assert_eq!(video.next_update().await,VideoUpdate::Connected);assert!(video.take_keyframe_request());
    let channel=incoming.recv().await.unwrap();
    loop {match channel.poll().await {Some(DataChannelEvent::OnOpen)=>break,other=>panic!("unexpected channel opening: {other:?}")}}
    assert_eq!(channel.label().await.unwrap(),super::super::receipts::LABEL);
    assert_eq!(channel.protocol().await.unwrap(),super::super::receipts::PROTOCOL);
    assert!(!channel.ordered().await.unwrap());assert_eq!(channel.max_retransmits().await.unwrap(),Some(0));
    assert_eq!(channel.max_packet_life_time().await.unwrap(),None);
    (video,receiver,channel,packets)
}
fn frame(pts:i64,keyframe:bool)->EncodedFrame {
    EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,if keyframe {0x65} else {0x41},0x88,0x84,0,0x31],keyframe,presentation_time_micros:pts}
}
async fn claim(channel:&Arc<dyn DataChannel>,stamp:VideoFrameStamp) {
    let mut bytes=bytes::BytesMut::with_capacity(12);bytes.extend_from_slice(b"FCD1");
    bytes.extend_from_slice(&(stamp.generation as u32).to_be_bytes());bytes.extend_from_slice(&stamp.rtp_timestamp.to_be_bytes());
    channel.send(bytes).await.unwrap();
}
async fn decoded(video:&mut WebRtcVideo)->DecodedFrameReceipt {
    loop {match video.next_update().await {
        VideoUpdate::FrameDecoded(receipt)=>return receipt,
        VideoUpdate::KeyframeRequested|VideoUpdate::BitrateTarget(_)=>{},
        other=>panic!("unexpected receipt update: {other:?}"),
    }}
}

#[test]
fn decoded_receipt_sctp_binds_written_frames_rejects_stale_claims_and_closes_optionally() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {tokio::time::timeout(Duration::from_secs(20),async {
        let (mut video,receiver,channel,mut packets)=pair().await;
        let first=video.send_with_stamp(frame(1_000_000,true)).await.unwrap();
        assert_eq!(packets.recv().await.unwrap().header.timestamp,first.rtp_timestamp);
        claim(&channel,first).await;
        assert_eq!(decoded(&mut video).await,DecodedFrameReceipt {stamp:first,presentation_time_micros:1_000_000});
        assert!(video.frame_receipts_available());
        claim(&channel,first).await;
        claim(&channel,VideoFrameStamp {generation:1,..first}).await;
        claim(&channel,VideoFrameStamp {rtp_timestamp:first.rtp_timestamp.wrapping_add(1),..first}).await;
        let second=video.send_with_stamp(frame(1_100_000,false)).await.unwrap();
        assert_eq!(packets.recv().await.unwrap().header.timestamp,second.rtp_timestamp);claim(&channel,second).await;
        assert_eq!(decoded(&mut video).await,DecodedFrameReceipt {stamp:second,presentation_time_micros:1_100_000});
        channel.close().await.unwrap();
        loop {match video.next_update().await {VideoUpdate::FrameReceiptsClosed=>break,VideoUpdate::KeyframeRequested|VideoUpdate::BitrateTarget(_)=>{},other=>panic!("unexpected close update: {other:?}")}}
        assert!(video.connected() && !video.frame_receipts_available());
        let third=video.send_with_stamp(frame(1_200_000,false)).await.unwrap();
        assert_eq!(packets.recv().await.unwrap().header.timestamp,third.rtp_timestamp);
        video.close();receiver.close().await.unwrap();
    }).await.expect("bounded receipt transport loopback");});
}

#[test]
fn decoded_receipt_malformed_wire_ends_only_its_own_stream() {
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {tokio::time::timeout(Duration::from_secs(20),async {
        let (mut video,receiver,channel,_packets)=pair().await;
        channel.send_text("FCD1").await.unwrap();
        loop {match video.next_update().await {VideoUpdate::Ended=>break,VideoUpdate::KeyframeRequested|VideoUpdate::BitrateTarget(_)=>{},other=>panic!("unexpected malformed update: {other:?}")}}
        assert!(video.ended());receiver.close().await.unwrap();
    }).await.expect("bounded malformed receipt loopback");});
}
