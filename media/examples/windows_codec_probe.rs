//! Bounded synthetic native encoder qualification. No screen capture or input.
#[cfg(target_os="windows")]
fn main()->Result<(),Box<dyn std::error::Error>> {
    use std::{fs::OpenOptions,io::Write,time::{Duration,Instant}};
    use voxl_media::{EncoderConfig,EncoderPreference,codec::VideoCodec,windows::{VideoEncoder,Nv12Color,EncodedOutput}};
    use ::windows::Win32::{Foundation::HMODULE,Graphics::{Direct3D::{D3D_DRIVER_TYPE_HARDWARE,D3D_FEATURE_LEVEL_11_0},Direct3D11::*,Dxgi::Common::{DXGI_FORMAT_NV12,DXGI_SAMPLE_DESC}}};
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=4 {return Err("Usage: windows_codec_probe software|hardware-memory|hardware-surface h264|hevc|av1 <new-output>".into());}
    let preference=match args[1].as_str() {"software"=>EncoderPreference::Software,"hardware-memory"|"hardware-surface"=>EncoderPreference::RequireHardware,_=>return Err("unknown encoder mode".into())};
    let codec=match args[2].as_str() {"h264"=>VideoCodec::H264,"hevc"=>VideoCodec::Hevc,"av1"=>VideoCodec::Av1,_=>return Err("unknown codec".into())};
    let config=EncoderConfig {codec,width:1280,height:720,fps:30,bitrate_kbps:2500,preference};
    let started=Instant::now();let mut device=None;
    if args[1]=="hardware-surface" {
        // SAFETY: writable device output, no swap chain/window, no WARP or
        // software adapter substitution. The encoder validates device identity.
        unsafe {D3D11CreateDevice(None,D3D_DRIVER_TYPE_HARDWARE,HMODULE::default(),D3D11_CREATE_DEVICE_VIDEO_SUPPORT|D3D11_CREATE_DEVICE_BGRA_SUPPORT,Some(&[D3D_FEATURE_LEVEL_11_0]),D3D11_SDK_VERSION,Some(&mut device),None,None)?;}
    }
    let mut encoder=if let Some(device)=&device {VideoEncoder::with_device(config,Nv12Color::Bt709Limited,device,||{})?} else {VideoEncoder::new(config,Nv12Color::Bt709Limited,||{})?};
    let setup_us=started.elapsed().as_micros();let observation=encoder.observation();
    println!("encoder codec={codec:?} hardware_registration={} device_input={} async={} setup_us={setup_us}",observation.hardware_registration,observation.device_input,observation.asynchronous);
    let mut output=OpenOptions::new().write(true).create_new(true).open(&args[3])?;
    if codec==VideoCodec::Av1 {
        output.write_all(b"DKIF\0\0\x20\0AV01")?;output.write_all(&(config.width as u16).to_le_bytes())?;output.write_all(&(config.height as u16).to_le_bytes())?;
        output.write_all(&config.fps.to_le_bytes())?;output.write_all(&1u32.to_le_bytes())?;output.write_all(&60u32.to_le_bytes())?;output.write_all(&0u32.to_le_bytes())?;
    }
    let width=config.width as usize;let height=config.height as usize;let mut pixels=vec![128u8;width*height*3/2];let mut leases=Vec::new();let mut total=0;let mut keyframes=0;
    for index in 0..60usize {
        // Deterministic limited-range luma with neutral chroma. The same input
        // is used for all modes/codecs. Per-frame upload is fixture overhead.
        for row in 0..height {for col in 0..width {pixels[row*width+col]=16+((col/16+row/16+index*3)%220)as u8;}}
        if !encoder.wait_ready(Duration::from_millis(200))? {return Err("encoder input deadline".into());}
        let before=Instant::now();let pts=index as i64*1_000_000/i64::from(config.fps);let force=index%30==0;
        if let Some(device)=&device {
            let desc=D3D11_TEXTURE2D_DESC {Width:config.width,Height:config.height,MipLevels:1,ArraySize:1,Format:DXGI_FORMAT_NV12,SampleDesc:DXGI_SAMPLE_DESC {Count:1,Quality:0},Usage:D3D11_USAGE_DEFAULT,BindFlags:D3D11_BIND_RENDER_TARGET.0 as u32,..Default::default()};
            let data=D3D11_SUBRESOURCE_DATA {pSysMem:pixels.as_ptr().cast(),SysMemPitch:config.width,SysMemSlicePitch:pixels.len()as u32};let mut texture=None;
            // SAFETY: CreateTexture2D copies the checked NV12 fixture at creation.
            // This texture is never mutated or recycled, even on submission error;
            // the sample retains it until the driver's tracked release callback.
            unsafe {device.CreateTexture2D(&desc,Some(&data),Some(&mut texture))?;leases.push(encoder.encode_surface(&texture.ok_or("missing texture")?,0,pts,force)?);}
        } else {encoder.encode_nv12(&pixels,width,pts,force)?;}
        let encoded=encoder.wait_output(Duration::from_millis(200))?.ok_or("encoder output deadline")?;
        let latency=before.elapsed().as_micros();let (data,keyframe,actual_pts)=match encoded {EncodedOutput::Nal(frame)=>(frame.annex_b,frame.keyframe,frame.presentation_time_micros),EncodedOutput::Av1(frame)=>(frame.temporal_unit,frame.keyframe,frame.presentation_time_micros)};
        if actual_pts!=pts || force && !keyframe {return Err("capture timestamp or forced keyframe mismatch".into());}
        if codec==VideoCodec::Av1 {output.write_all(&(data.len()as u32).to_le_bytes())?;output.write_all(&(index as u64).to_le_bytes())?;}
        output.write_all(&data)?;total+=data.len();keyframes+=usize::from(keyframe);
        println!("frame={index} pts_us={pts} bytes={} keyframe={keyframe} submit_to_output_us={latency}",data.len());
    }
    encoder.close();output.sync_all()?;
    println!("complete frames=60 bytes={total} keyframes={keyframes} released_surfaces={} retained_surfaces={} capture=false receiver=false performance_qualified=false",leases.iter().filter(|lease|lease.released()).count(),leases.iter().filter(|lease|!lease.released()).count());
    Ok(())
}
#[cfg(not(target_os="windows"))]
fn main() {eprintln!("windows_codec_probe requires native Windows; cross-checking does not run an encoder");std::process::exit(2);}
