use super::*;
use crate::{EncoderConfig,EncoderPreference};
use super::super::{VideoEncoder,compare_frames,FrameObserver,FrameComparison};
use std::time::Duration;

fn annex(nals:&[&[u8]])->Vec<u8> {let mut out=Vec::new();for nal in nals {out.extend_from_slice(&[0,0,0,1]);out.extend_from_slice(nal);}out}
fn frame(bytes:Vec<u8>)->EncodedFrame {EncodedFrame {codec:VideoCodec::H264,presentation_time_micros:0,keyframe:true,annex_b:bytes}}
#[test]
fn native_source_quality_framing_has_explicit_caps_and_exact_nal_boundaries() {
    let data=annex(&[&[0x67,1,2],&[0x68,3],&[0x65,4,0,0,3,1],&[0x06,5]]);
    let parsed=AccessUnit::read(&data).unwrap();assert!(parsed.idr);assert_eq!(parsed.nals.len(),4);
    assert_eq!(parsed.parameters.as_ref().unwrap().sps,[0x67,1,2]);
    assert_eq!(crate::nal_units_to_annex_b(&parsed.length_prefixed(),4,VideoCodec::H264).unwrap(),(data,true));
    for data in [vec![],vec![0,0,1,0x65],annex(&[&[]]),annex(&[&[0xe5,1]]),annex(&[&[0x67,1],&[0x65,1]]),annex(&[&[0x67,1],&[0x68,1]]),annex(&[&[0x67,1],&[0x67,1],&[0x68,1],&[0x65,1]]),annex(&[&[0x02,1]])] {assert!(AccessUnit::read(&data).is_err());}
    let too_many:Vec<&[u8]>=std::iter::repeat_n([0x61,1].as_slice(),MAX_NALS+1).collect();assert!(AccessUnit::read(&annex(&too_many)).is_err());
    let oversized=vec![0x67;MAX_PARAMETERS+1];assert!(matches!(AccessUnit::read(&annex(&[&oversized,&[0x68,1],&[0x65,1]])),Err(MediaError::Backpressure)));
    assert!(AccessUnit::read(&vec![0;MAX_FRAME_BYTES+1]).is_err());
}
#[test]
fn native_source_quality_bounds_and_errors_retire_the_chain_without_native_work() {
    for (width,height) in [(0,180),(321,180),(320,181),(4096,2160),(320,2162)] {assert!(H264QualityDecoder::new(width,height,true,||{}).is_err());}
    for sample in [frame(vec![]),frame(annex(&[&[0x61,1]])),EncodedFrame {codec:VideoCodec::Hevc,..frame(annex(&[&[0x65,1]]))},EncodedFrame {presentation_time_micros:-1,..frame(annex(&[&[0x65,1]]))}] {
        let mut decoder=H264QualityDecoder::new(320,180,true,||{}).unwrap();assert!(decoder.decode(&sample).is_err());assert!(matches!(decoder.decode(&sample),Err(MediaError::Closed)));
    }
    let mut decoder=H264QualityDecoder::new(320,180,true,||{}).unwrap();decoder.close();assert!(matches!(decoder.decode(&frame(vec![])),Err(MediaError::Closed)));
}
#[test]
fn native_source_quality_callbacks_fail_closed_on_error_drop_duplicate_or_hook_panic() {
    for (status,flags) in [(-1,0),(0,2),(0,0)] {
        let slot=Slot {output:Mutex::new(Output::Waiting),idle:||{}};
        // SAFETY: the live local Slot is synchronized, and error paths must not
        // dereference the deliberately null image in this callback contract.
        unsafe {decoded(ptr::null_mut(),(&slot as *const Slot).cast_mut().cast(),status,flags,ptr::null(),CMTime {value:0,timescale:1,flags:CMTimeFlags::Valid,epoch:0},CMTime {value:0,timescale:0,flags:CMTimeFlags::empty(),epoch:0});}
        assert!(matches!(*slot.output.lock().unwrap(),Output::Failed(_)));
    }
    let slot=Slot {output:Mutex::new(Output::Failed(MediaError::Closed)),idle:||{}};
    // SAFETY: a duplicate completion cannot dereference the absent image.
    unsafe {decoded(ptr::null_mut(),(&slot as *const Slot).cast_mut().cast(),0,0,ptr::null(),CMTime {value:0,timescale:1,flags:CMTimeFlags::Valid,epoch:0},CMTime {value:0,timescale:0,flags:CMTimeFlags::empty(),epoch:0});}
    assert!(matches!(*slot.output.lock().unwrap(),Output::Failed(MediaError::InvalidFrame)));
    let slot=Slot {output:Mutex::new(Output::Waiting),idle:||panic!("intentional qualifier hook failure")};
    // SAFETY: the callback contains the deliberately panicking embedding hook.
    unsafe {decoded(ptr::null_mut(),(&slot as *const Slot).cast_mut().cast(),-1,0,ptr::null(),CMTime {value:0,timescale:1,flags:CMTimeFlags::Valid,epoch:0},CMTime {value:0,timescale:0,flags:CMTimeFlags::empty(),epoch:0});}
    assert!(matches!(*slot.output.lock().unwrap(),Output::Failed(MediaError::Closed)));
}

#[link(name="CoreVideo",kind="framework")]
unsafe extern "C" {
    fn CVPixelBufferCreate(allocator:Object,width:usize,height:usize,format:u32,attributes:Object,out:*mut Object)->i32;
    fn CVPixelBufferLockBaseAddress(buffer:Object,flags:u64)->i32;
    fn CVPixelBufferUnlockBaseAddress(buffer:Object,flags:u64)->i32;
    fn CVPixelBufferGetBaseAddressOfPlane(buffer:Object,plane:usize)->*mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer:Object,plane:usize)->usize;
    fn CVBufferSetAttachment(buffer:Object,key:Object,value:Object,mode:u32);
    static kCVImageBufferColorPrimaries_ITU_R_709_2:Object;
    static kCVImageBufferTransferFunction_sRGB:Object;
    static kCVImageBufferYCbCrMatrix_ITU_R_709_2:Object;
}
fn image()->PixelBuffer {
    let mut raw=ptr::null();
    // SAFETY: this Create reference is exclusively written and tagged before
    // entering the immutable wrapper. Its original reference is then released.
    unsafe {
        assert_eq!(CVPixelBufferCreate(ptr::null(),320,180,u32::from_be_bytes(*b"420f"),ptr::null(),&mut raw),0);assert!(!raw.is_null());
        assert_eq!(CVPixelBufferLockBaseAddress(raw,0),0);
        for (plane,rows) in [(0,180),(1,90)] {
            let base=CVPixelBufferGetBaseAddressOfPlane(raw,plane);let stride=CVPixelBufferGetBytesPerRowOfPlane(raw,plane);assert!(!base.is_null() && stride>=320);
            for row in 0..rows {for column in 0..320 {*base.add(row*stride+column)=if plane==0 {if (row/9+column/7)%2==0 {48} else {196}} else {128};}}
        }
        assert_eq!(CVPixelBufferUnlockBaseAddress(raw,0),0);
        for (key,value) in [(kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_709_2),(kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_sRGB),(kCVImageBufferYCbCrMatrixKey,kCVImageBufferYCbCrMatrix_ITU_R_709_2)] {CVBufferSetAttachment(raw,key,value,1);}
        let image=PixelBuffer::from_raw(raw).unwrap();CFRelease(raw);image
    }
}
#[test]
fn native_source_quality_actual_software_decode_preserves_pts_range_and_retained_images() {
    let config=EncoderConfig {codec:VideoCodec::H264,width:320,height:180,fps:30,bitrate_kbps:4000,preference:EncoderPreference::Software};
    let mut encoder=VideoEncoder::new(config,||{}).unwrap();assert!(!encoder.using_hardware());
    let mut decoder=H264QualityDecoder::new(320,180,true,||{}).unwrap();assert_eq!(decoder.parameter_signal(),None);let source=image();let mut retained=None;let mut observer=FrameObserver::new(320*180*3/2).unwrap();
    for index in 0..4 {
        let pts=index*33333;encoder.encode(&source,pts,index==0).unwrap();let output=encoder.wait_output(Duration::from_millis(500)).unwrap().expect("actual encoder output");assert_eq!(output.presentation_time_micros,pts);
        let decoded=decoder.decode(&output).unwrap();
        let signal=decoder.parameter_signal().expect("active encoded SPS metadata");assert_eq!(signal.video_signal,Some(crate::h264::VideoSignal {video_format:5,full_range:true,colour_description:Some([1,13,1])}));
        let difference=compare_frames(&source,&decoded).unwrap();assert!(difference.color_tags_complete);assert_eq!(difference.planes.map(|plane|plane.samples),[57600,14400,14400]);
        if index==0 {assert_eq!(observer.observe(&decoded),Ok(FrameComparison::Initial));retained=Some(decoded);}
    }
    assert_eq!(observer.observe(retained.as_ref().unwrap()),Ok(FrameComparison::Equal));
    let repeated=EncodedFrame {presentation_time_micros:33333,..frame(annex(&[&[0x61,1]]))};
    assert!(decoder.decode(&repeated).is_err());assert!(matches!(decoder.decode(&repeated),Err(MediaError::Closed)));assert_eq!(decoder.parameter_signal(),None);
}
