//! Local interoperability fixture: replay complete native access units through
//! the actual transport. File IPC keeps signaling outside the media library.
//! It captures no screen, accepts no remote control and has no relay credentials.
#[cfg(feature = "webrtc")]
mod enabled {
    use std::{error::Error, fs, path::Path, time::Duration};
    use voxl_media::{EncodedFrame, codec::VideoCodec, webrtc::{MediaRoute, VideoFormat, VideoUpdate, WebRtcConfig, WebRtcVideo}};

    fn frames(path: &Path) -> Result<Vec<EncodedFrame>, Box<dyn Error>> {
        if fs::metadata(path)?.len() > 64 * 1024 * 1024 { return Err("fixture exceeds limit".into()); }
        let data = fs::read(path)?;
        if data.get(..5) != Some(b"VXFR\x01") { return Err("fixture header".into()); }
        let codec = match data.get(5) { Some(0) => VideoCodec::H264, Some(1) => VideoCodec::Hevc, _ => return Err("fixture codec".into()) };
        let mut offset = 6;
        let mut frames = Vec::new();
        while offset < data.len() {
            if frames.len() >= 300 { return Err("fixture frame count".into()); }
            let header = data.get(offset..offset+13).ok_or("truncated frame header")?;
            let pts = i64::from_be_bytes(header[..8].try_into()?);
            let keyframe = match header[8] { 0 => false, 1 => true, _ => return Err("fixture keyframe".into()) };
            let length = u32::from_be_bytes(header[9..13].try_into()?) as usize;
            if length == 0 || length > 8 * 1024 * 1024 || pts < 0 { return Err("fixture frame bounds".into()); }
            offset += 13;
            let annex_b = data.get(offset..offset+length).ok_or("truncated frame")?.to_vec();
            offset += length;
            frames.push(EncodedFrame {codec,annex_b,keyframe,presentation_time_micros: pts});
        }
        if frames.first().is_none_or(|frame| !frame.keyframe) { return Err("fixture must begin with a keyframe".into()); }
        Ok(frames)
    }

    pub async fn run() -> Result<(), Box<dyn Error>> {
        let args: Vec<_> = std::env::args().collect();
        if args.len() != 5 { return Err("Usage: encoded_peer_probe <frames> <offer.sdp> <answer.sdp> <sender-done>".into()); }
        let frames = frames(Path::new(&args[1]))?;
        let first = &frames[0];
        let format = match first.codec {
            VideoCodec::H264 => VideoFormat::H264 {profile_level_id: voxl_media::h264_profile_level_id(&first.annex_b)?},
            VideoCodec::Hevc => VideoFormat::HevcMain(voxl_media::hevc_main_profile(&first.annex_b)?),
            VideoCodec::Av1 => return Err("AV1 fixture is not implemented".into()),
        };
        let mut video = Box::pin(WebRtcVideo::open(WebRtcConfig::direct_first(format,vec!["127.0.0.1".into()]))).await?;
        let temporary = format!("{}.tmp",args[2]);
        fs::write(&temporary,video.offer())?;
        fs::rename(temporary,&args[2])?;
        let answer = tokio::time::timeout(Duration::from_secs(30),async {
            loop {
                match fs::metadata(&args[3]) {
                    Ok(meta) => {
                        if meta.len() > 256 * 1024 { return Err("answer exceeds limit".into()); }
                        return fs::read_to_string(&args[3]).map_err(Box::<dyn Error>::from);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => tokio::time::sleep(Duration::from_millis(25)).await,
                    Err(error) => return Err(error.into()),
                }
            }
        }).await??;
        video.answer(answer).await?;
        tokio::time::timeout(Duration::from_secs(20),async {
            loop { match video.next_update().await {
                VideoUpdate::Connected => return Ok::<_,Box<dyn Error>>(()),
                VideoUpdate::KeyframeRequested => {},
                VideoUpdate::Ended => return Err("transport ended before connection".into()),
            } }
        }).await??;
        let started = tokio::time::Instant::now();
        let count = frames.len();
        let first_pts = frames[0].presentation_time_micros;
        for frame in frames {
            let elapsed = frame.presentation_time_micros.checked_sub(first_pts).ok_or("fixture timestamp order")?;
            let elapsed = u64::try_from(elapsed)?;
            tokio::time::sleep_until(started+Duration::from_micros(elapsed)).await;
            video.send(frame).await?;
        }
        let route = video.route_statistics().await;
        if route.route != MediaRoute::Direct || route.bytes_sent == 0 { return Err("fixture did not prove direct media".into()); }
        println!("sent_frames={count} route={route:?}");
        fs::write(&args[4],b"sent")?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        video.close();
        Ok(())
    }
}

#[cfg(feature = "webrtc")]
fn main() -> Result<(),Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(enabled::run())
}
#[cfg(not(feature = "webrtc"))]
fn main() { eprintln!("The encoded peer fixture requires --features webrtc."); }
