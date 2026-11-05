//! Bounded synthetic codec probe. It captures no screen and records no user
//! input. Its deterministic NV12 frames can be decoded independently by ffmpeg.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{ffi::c_void, ptr, time::{Duration, Instant}, io::Write};
    use voxl_media::{EncoderConfig, EncoderPreference, EncoderProfile, EncoderRateControl, EncoderTuning};
    use voxl_media::codec::VideoCodec;
    use voxl_media::macos::{VideoEncoder, PixelBuffer};
    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVPixelBufferCreate(allocator: *const c_void, width: usize, height: usize, format: u32, attrs: *const c_void, out: *mut *const c_void) -> i32;
        fn CVPixelBufferLockBaseAddress(buffer: *const c_void, flags: u64) -> i32;
        fn CVPixelBufferUnlockBaseAddress(buffer: *const c_void, flags: u64) -> i32;
        fn CVPixelBufferGetBaseAddressOfPlane(buffer: *const c_void, plane: usize) -> *mut u8;
        fn CVPixelBufferGetBytesPerRowOfPlane(buffer: *const c_void, plane: usize) -> usize;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" { fn CFRelease(object: *const c_void); }
    fn option(args: &mut Vec<String>, prefix: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let indices: Vec<_> = args.iter().enumerate().filter_map(|(index,arg)| arg.starts_with(prefix).then_some(index)).collect();
        if indices.len() > 1 { return Err("duplicate codec probe option".into()); }
        Ok(indices.first().map(|index| args.remove(*index)[prefix.len()..].to_owned()))
    }
    let mut args: Vec<_> = std::env::args().collect();
    let tuning_name = option(&mut args,"--tuning=")?.unwrap_or_else(||"compatible".to_owned());
    let tuning = match tuning_name.as_str() {
        "compatible" => EncoderTuning::default(),
        "high" => EncoderTuning { rate_control: EncoderRateControl::RealTime, profile: EncoderProfile::H264High },
        "constrained-high" => EncoderTuning { rate_control: EncoderRateControl::RealTime, profile: EncoderProfile::H264ConstrainedHigh },
        "low-latency-compatible" => EncoderTuning { rate_control: EncoderRateControl::LowLatency, profile: EncoderProfile::Compatible },
        "low-latency-high" => EncoderTuning { rate_control: EncoderRateControl::LowLatency, profile: EncoderProfile::H264High },
        "low-latency-constrained-high" => EncoderTuning { rate_control: EncoderRateControl::LowLatency, profile: EncoderProfile::H264ConstrainedHigh },
        _ => return Err("unknown codec probe tuning".into()),
    };
    let parse_count = |value: Option<String>, default: usize| -> Result<usize, Box<dyn std::error::Error>> {
        value.map(|value|value.parse::<usize>()).transpose().map(|value|value.unwrap_or(default)).map_err(Into::into)
    };
    let warmup = parse_count(option(&mut args,"--warmup-frames=")?,0)?;
    let count = parse_count(option(&mut args,"--frames=")?,60)?;
    let force_period = option(&mut args,"--force-keyframe-period=")?.map(|value|value.parse::<usize>()).transpose()?;
    if warmup > 1800 || !(1..=18000).contains(&count) || force_period.is_some_and(|period|!(1..=1800).contains(&period))
        || !(3..=5).contains(&args.len()) || args.iter().skip(1).any(|arg|arg.starts_with("--"))
    { return Err("codec probe options exceed their bounds".into()); }
    let preference = match args.get(1).map(String::as_str) {
        Some("hardware") => EncoderPreference::RequireHardware,
        Some("software") => EncoderPreference::Software,
        _ => return Err("Usage: codec_probe hardware|software <output> [h264|hevc] [framed-output] [--tuning=compatible|high|constrained-high|low-latency-compatible|low-latency-high|low-latency-constrained-high] [--warmup-frames=N] [--frames=N] [--force-keyframe-period=N]".into()),
    };
    let path = args.get(2).ok_or("output path required")?;
    let codec = match args.get(3).map(String::as_str) {
        None | Some("h264") => VideoCodec::H264,
        Some("hevc") => VideoCodec::Hevc,
        _ => return Err("codec must be h264 or hevc".into()),
    };
    let config = EncoderConfig { codec, width: 1280, height: 720, fps: 30, bitrate_kbps: 2500, preference };
    tuning.validate(&config)?;
    let create = |path| std::fs::OpenOptions::new().write(true).create_new(true).open(path);
    let mut file = create(path)?;
    // Test-only access-unit capture preserves frame boundaries and timestamps
    // for native browser interoperability; it is not a recording format API.
    let mut framed = args.get(4).map(create).transpose()?;
    if let Some(framed) = &mut framed {
        framed.write_all(b"VXFR\x01")?;
        framed.write_all(&[if codec == VideoCodec::H264 { 0 } else { 1 }])?;
    }
    println!("requested_tuning={tuning_name} warmup_frames={warmup} observed_frames={count} force_keyframe_period={force_period:?}");
    let mut encoder = VideoEncoder::new_tuned(config,tuning,|| {})?;
    println!("hardware_selection={:?}",encoder.acceleration_selection());
    let started = Instant::now();
    let mut bytes = 0;
    let mut total_bytes = 0;
    let mut latencies = Vec::new();
    let mut keyframes = 0;
    let mut keyframe_indices = Vec::new();
    let mut first_frame_ms = None;
    for index in 0..warmup+count {
        let deadline = started + Duration::from_secs_f64(index as f64 / 30.0);
        if let Some(wait) = deadline.checked_duration_since(Instant::now()) { std::thread::sleep(wait); }
        let mut raw = ptr::null();
        let code = unsafe { CVPixelBufferCreate(ptr::null(),1280,720,u32::from_be_bytes(*b"420v"),ptr::null(),&mut raw) };
        if code != 0 || raw.is_null() { return Err(format!("pixel buffer creation: {code}").into()); }
        unsafe {
            if CVPixelBufferLockBaseAddress(raw,0) != 0 { CFRelease(raw); return Err("pixel buffer lock failed".into()); }
            let y_stride = CVPixelBufferGetBytesPerRowOfPlane(raw,0);
            let uv_stride = CVPixelBufferGetBytesPerRowOfPlane(raw,1);
            let y = CVPixelBufferGetBaseAddressOfPlane(raw,0);
            let uv = CVPixelBufferGetBaseAddressOfPlane(raw,1);
            if y.is_null() || uv.is_null() {
                CVPixelBufferUnlockBaseAddress(raw,0); CFRelease(raw); return Err("missing NV12 planes".into());
            }
            for row in 0..720usize {
                for col in 0..1280usize {
                    // Small text-like edges and a scrolling bar over a stable
                    // background exercise interframe coding deterministically.
                    let bar = (col + index*8) % 1280 < 32;
                    let grid = row % 24 < 2 && col % 12 < 8;
                    y.add(row*y_stride+col).write(if bar {200} else if grid {40} else {100});
                }
            }
            for row in 0..360usize { ptr::write_bytes(uv.add(row*uv_stride),128,1280); }
            CVPixelBufferUnlockBaseAddress(raw,0);
        }
        let image = unsafe { PixelBuffer::from_raw(raw)? };
        unsafe { CFRelease(raw); }
        let submitted = Instant::now();
        let forced = index==0 || force_period.is_some_and(|period|index.is_multiple_of(period));
        encoder.encode(&image,(index as i64)*1_000_000/30,forced)?;
        // Use the native callback wake, as the application's dedicated worker
        // does. Record arrival before logging or writing the encoded stream so
        // the measured interval excludes file and profile-output overhead.
        let output = encoder.wait_output(Duration::from_secs(2))?.ok_or("encoder output deadline")?;
        let latency = submitted.elapsed().as_secs_f64()*1000.0;
        if forced && !output.keyframe { return Err("forced keyframe was not observed".into()); }
        if index == 0 {
            first_frame_ms = Some(latency);
            match codec {
                VideoCodec::H264 => println!("profile_level_id={:02x?}",voxl_media::h264_profile_level_id(&output.annex_b)?),
                VideoCodec::Hevc => println!("hevc_fmtp={}",voxl_media::hevc_main_profile(&output.annex_b)?.fmtp()),
                VideoCodec::Av1 => unreachable!(),
            }
        }
        if output.codec != codec { return Err("codec mismatch".into()); }
        if output.presentation_time_micros != (index as i64)*1_000_000/30 { return Err("timestamp mismatch".into()); }
        total_bytes += output.annex_b.len();
        if output.keyframe { keyframe_indices.push(index); }
        if index>=warmup {bytes += output.annex_b.len();keyframes += usize::from(output.keyframe);latencies.push(latency);}
        file.write_all(&output.annex_b)?;
        if let Some(framed) = &mut framed {
            framed.write_all(&output.presentation_time_micros.to_be_bytes())?;
            framed.write_all(&[u8::from(output.keyframe)])?;
            framed.write_all(&u32::try_from(output.annex_b.len())?.to_be_bytes())?;
            framed.write_all(&output.annex_b)?;
        }
    }
    println!("wait_strategy=callback latency_scope=encode_call_to_output_received first_frame_ms={:.2}",first_frame_ms.ok_or("first frame missing")?);
    println!("encode_samples_ms={latencies:?}");
    println!("keyframe_indices={keyframe_indices:?} total_stream_bytes={total_bytes}");
    latencies.sort_by(f64::total_cmp);
    let rank = |percent: usize| latencies[(count*percent).div_ceil(100)-1];
    println!("codec={codec:?} hardware={} width=1280 height=720 fps=30 frames={count} warmup_frames={warmup} keyframes={keyframes} bytes={bytes} encoded_kbps={:.1} encode_p50_ms={:.2} encode_p95_ms={:.2} wall_seconds={:.3} percentile=nearest_rank bitrate_duration=nominal_observed_frames_over_fps",
        encoder.using_hardware(), bytes as f64*8.0*30.0/(count as f64*1000.0), rank(50),rank(95),started.elapsed().as_secs_f64());
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() { eprintln!("The native codec probe requires macOS."); }
