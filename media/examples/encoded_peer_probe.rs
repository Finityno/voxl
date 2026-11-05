//! Local interoperability fixture: replay complete native access units through
//! the actual transport. File IPC keeps signaling outside the media library.
//! It captures no screen, accepts no remote control and has no relay credentials.
#[cfg(feature = "webrtc")]
mod enabled {
    use std::{error::Error, fs, path::Path, time::Duration};
    use voxl_media::{av1::{Av1Frame,av1_main_profile},EncodedFrame, codec::VideoCodec, webrtc::{MediaRoute, VideoFormat, VideoUpdate, WebRtcConfig, WebRtcVideo}};

    enum Frame {Nal(EncodedFrame),Av1(Av1Frame)}
    impl Frame {
        fn pts(&self)->i64 {match self {Self::Nal(frame)=>frame.presentation_time_micros,Self::Av1(frame)=>frame.presentation_time_micros}}
        fn keyframe(&self)->bool {match self {Self::Nal(frame)=>frame.keyframe,Self::Av1(frame)=>frame.keyframe}}
        fn format(&self)->Result<VideoFormat,Box<dyn Error>> {
            Ok(match self {
                Self::Av1(frame)=>VideoFormat::Av1Main(av1_main_profile(&frame.temporal_unit)?),
                Self::Nal(frame)=>match frame.codec {
                    VideoCodec::H264=>VideoFormat::H264 {profile_level_id:voxl_media::h264_profile_level_id(&frame.annex_b)?},
                    VideoCodec::Hevc=>VideoFormat::HevcMain(voxl_media::hevc_main_profile(&frame.annex_b)?),
                    VideoCodec::Av1=>return Err("AV1 requires sized OBUs".into()),
                },
            })
        }
    }
    fn ivf(data:&[u8])->Result<Vec<Frame>,Box<dyn Error>> {
        if data.len()<32 || data[4..8]!=[0,0,32,0] || &data[8..12]!=b"AV01" {return Err("AV1 IVF header".into());}
        let rate=u32::from_le_bytes(data[16..20].try_into()?);let scale=u32::from_le_bytes(data[20..24].try_into()?);
        if rate==0 || scale==0 {return Err("AV1 IVF timebase".into());}
        let mut frames=Vec::new();let mut offset=32;
        while offset<data.len() {
            if frames.len()>=300 {return Err("AV1 IVF frame count".into());}
            let header=data.get(offset..offset+12).ok_or("truncated AV1 IVF frame header")?;offset+=12;
            let length=u32::from_le_bytes(header[..4].try_into()?)as usize;
            if length==0 || length>8*1024*1024 {return Err("AV1 IVF frame bounds".into());}
            let timestamp=u64::from_le_bytes(header[4..12].try_into()?);
            let presentation_time_micros=i64::try_from(i128::from(timestamp)*i128::from(scale)*1_000_000/i128::from(rate))?;
            let temporal_unit=data.get(offset..offset+length).ok_or("truncated AV1 IVF frame")?.to_vec();offset+=length;
            let keyframe=av1_main_profile(&temporal_unit).is_ok();
            frames.push(Frame::Av1(Av1Frame {temporal_unit,keyframe,presentation_time_micros}));
        }
        if frames.first().is_none_or(|frame|!frame.keyframe()) {return Err("AV1 IVF must begin with a random-access unit".into());}
        Ok(frames)
    }
    fn frames(path: &Path) -> Result<Vec<Frame>, Box<dyn Error>> {
        if fs::metadata(path)?.len() > 64 * 1024 * 1024 { return Err("fixture exceeds limit".into()); }
        let data = fs::read(path)?;
        if data.get(..4)==Some(b"DKIF") {return ivf(&data);}
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
            frames.push(Frame::Nal(EncodedFrame {codec,annex_b,keyframe,presentation_time_micros: pts}));
        }
        if frames.first().is_none_or(|frame| !frame.keyframe()) { return Err("fixture must begin with a keyframe".into()); }
        Ok(frames)
    }

    pub async fn run() -> Result<(), Box<dyn Error>> {
        let args: Vec<_> = std::env::args().collect();
        if args.len() != 5 { return Err("Usage: encoded_peer_probe <frames> <offer.sdp> <answer.sdp> <sender-done>".into()); }
        let frames = frames(Path::new(&args[1]))?;
        let format=frames[0].format()?;
        println!("loaded_frames={} format={format:?}",frames.len());
        let mut video = Box::pin(WebRtcVideo::open(WebRtcConfig::direct_first(format,vec!["127.0.0.1:0".into()]))).await?;
        let temporary = format!("{}.tmp",args[2]);
        fs::write(&temporary,video.offer())?;
        fs::rename(temporary,&args[2])?;
        println!("offer_ready");
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
        println!("answer_accepted");
        tokio::time::timeout(Duration::from_secs(20),async {
            loop { match video.next_update().await {
                VideoUpdate::Connected => return Ok::<_,Box<dyn Error>>(()),
                VideoUpdate::KeyframeRequested => {},
                VideoUpdate::Disconnected|VideoUpdate::Ended => return Err("transport ended before connection".into()),
            } }
        }).await??;
        let started = tokio::time::Instant::now();
        let count = frames.len();
        let first_pts = frames[0].pts();
        for frame in frames {
            let elapsed = frame.pts().checked_sub(first_pts).ok_or("fixture timestamp order")?;
            let elapsed = u64::try_from(elapsed)?;
            tokio::time::sleep_until(started+Duration::from_micros(elapsed)).await;
            match frame {Frame::Nal(frame)=>video.send(frame).await?,Frame::Av1(frame)=>video.send_av1(frame).await?,}
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
