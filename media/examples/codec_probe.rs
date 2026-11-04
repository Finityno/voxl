//! Bounded synthetic codec probe. It captures no screen and records no user
//! input. Its deterministic NV12 frames can be decoded independently by ffmpeg.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{ffi::c_void, ptr, time::{Duration, Instant}, io::Write};
    use voxl_media::{EncoderConfig, EncoderPreference};
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
    let args: Vec<_> = std::env::args().collect();
    let preference = match args.get(1).map(String::as_str) {
        Some("hardware") => EncoderPreference::RequireHardware,
        Some("software") => EncoderPreference::Software,
        _ => return Err("Usage: codec_probe hardware|software <output> [h264|hevc]".into()),
    };
    let path = args.get(2).ok_or("output path required")?;
    let mut file = std::fs::File::create(path)?;
    let codec = match args.get(3).map(String::as_str) {
        None | Some("h264") => VideoCodec::H264,
        Some("hevc") => VideoCodec::Hevc,
        _ => return Err("codec must be h264 or hevc".into()),
    };
    let config = EncoderConfig { codec, width: 1280, height: 720, fps: 30, bitrate_kbps: 2500, preference };
    let mut encoder = VideoEncoder::new(config, || {})?;
    let started = Instant::now();
    let mut bytes = 0;
    let mut latencies = Vec::new();
    let mut keyframes = 0;
    for index in 0..60usize {
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
        encoder.encode(&image,(index as i64)*1_000_000/30,index==0)?;
        let limit = submitted + Duration::from_secs(2);
        let output = loop {
            if let Some(frame) = encoder.take_output()? { break frame; }
            if Instant::now() > limit { return Err("encoder output deadline".into()); }
            std::thread::sleep(Duration::from_millis(1));
        };
        if output.codec != codec { return Err("codec mismatch".into()); }
        if output.presentation_time_micros != (index as i64)*1_000_000/30 { return Err("timestamp mismatch".into()); }
        bytes += output.annex_b.len();
        keyframes += usize::from(output.keyframe);
        file.write_all(&output.annex_b)?;
        latencies.push(submitted.elapsed().as_secs_f64()*1000.0);
    }
    latencies.sort_by(f64::total_cmp);
    println!("codec={codec:?} hardware={} width=1280 height=720 fps=30 frames=60 keyframes={keyframes} bytes={bytes} encoded_kbps={:.1} encode_p50_ms={:.2} encode_p95_ms={:.2} wall_seconds={:.3}",
        encoder.using_hardware(), bytes as f64*8.0/2000.0, latencies[30],latencies[57],started.elapsed().as_secs_f64());
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() { eprintln!("The native codec probe requires macOS."); }
