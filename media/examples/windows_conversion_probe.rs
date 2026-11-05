//! Numeric synthetic conversion oracle. No screen capture, window or OS input.
#[cfg(target_os="windows")]
fn main()->Result<(),Box<dyn std::error::Error>> {
    use std::{fs::OpenOptions,io::Write,rc::Rc};
    use voxl_media::{EncoderConfig,EncoderPreference,codec::VideoCodec,windows::{capture::CaptureDevice,convert::Nv12Converter}};
    use ::windows::Win32::Graphics::{Direct3D11::*,Dxgi::Common::*};
    let args:Vec<_>=std::env::args().collect();if args.len()!=2 {return Err("Usage: windows_conversion_probe <new-nv12-output>".into());}
    let device=CaptureDevice::default_hardware()?;
    let config=EncoderConfig {codec:VideoCodec::H264,width:640,height:360,fps:30,bitrate_kbps:2500,preference:EncoderPreference::Software};
    let mut converter=Nv12Converter::new(Rc::clone(&device),(1280,720),config,||{})?;
    // Eight exact RGB primaries/secondaries: an independent BT.709 limited
    // matrix oracle. Interior samples avoid resampling/chroma-boundary edges.
    let colors=[[0,0,0],[255,255,255],[255,0,0],[0,255,0],[0,0,255],[0,255,255],[255,0,255],[255,255,0]];
    let expected=[[16,128,128],[235,128,128],[63,102,240],[173,42,26],[32,240,118],[188,154,16],[78,214,230],[219,16,138]];
    let mut bgra=vec![0u8;1280*720*4];
    for row in 0..720 {for col in 0..1280 {let [r,g,b]=colors[col/160];let at=(row*1280+col)*4;bgra[at..at+4].copy_from_slice(&[b,g,r,255]);}}
    let desc=D3D11_TEXTURE2D_DESC {Width:1280,Height:720,MipLevels:1,ArraySize:1,Format:DXGI_FORMAT_B8G8R8A8_UNORM,SampleDesc:DXGI_SAMPLE_DESC {Count:1,Quality:0},Usage:D3D11_USAGE_DEFAULT,BindFlags:D3D11_BIND_RENDER_TARGET.0 as u32,..Default::default()};
    let input=D3D11_SUBRESOURCE_DATA {pSysMem:bgra.as_ptr().cast(),SysMemPitch:1280*4,SysMemSlicePitch:bgra.len()as u32};let mut texture=None;
    // SAFETY: texture creation copies the full source. This fixture exclusively
    // owns it, never mutates/recycles it, and conversion waits for GPU completion.
    let data=unsafe {device.d3d11().CreateTexture2D(&desc,Some(&input),Some(&mut texture))?;converter.fixture_nv12(&texture.ok_or("missing fixture texture")?)?};
    if data.len()!=640*360*3/2 {return Err("NV12 byte geometry mismatch".into());}
    let mut max_error=0;
    for (bar,expected) in expected.iter().enumerate() {
        let x=bar*80+40;let uv=640*360+90*640+x;
        let actual=[data[180*640+x],data[uv],data[uv+1]];
        for (a,b) in actual.iter().zip(expected) {max_error=max_error.max(i32::from(*a).abs_diff(*b));}
        println!("bar={bar} yuv={actual:?} expected={expected:?}");
    }
    if max_error>3 {return Err(format!("BT.709 limited conversion differs by {max_error} codes").into());}
    let mut output=OpenOptions::new().write(true).create_new(true).open(&args[1])?;output.write_all(&data)?;output.sync_all()?;
    println!("complete bars=8 max_code_error={max_error} input=1280x720 output=640x360 bytes={} capture=false encoder=false performance_qualified=false",data.len());Ok(())
}
#[cfg(not(target_os="windows"))]
fn main() {eprintln!("windows_conversion_probe requires native Windows; cross-checking does not convert pixels");std::process::exit(2);}
