use super::*;
use std::time::Duration;

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferCreate(allocator: Object, width: usize, height: usize, format: u32, attributes: Object, out: *mut Object) -> i32;
    fn CVPixelBufferLockBaseAddress(buffer: Object, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: Object, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddressOfPlane(buffer: Object, plane: usize) -> *mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer: Object, plane: usize) -> usize;
    fn CVBufferSetAttachment(buffer: Object, key: Object, value: Object, mode: u32);
    static kCVImageBufferColorPrimaries_ITU_R_709_2: Object;
    static kCVImageBufferTransferFunction_ITU_R_709_2: Object;
    static kCVImageBufferTransferFunction_sRGB: Object;
    static kCVImageBufferYCbCrMatrix_ITU_R_709_2: Object;
}

fn image(full: bool, transfer: Option<Object>) -> PixelBuffer {
    let mut raw = ptr::null();
    // SAFETY: this fixture owns the buffer while filling pixels and attaching
    // live CF values. After from_raw all pixels/attachments are immutable.
    unsafe {
        assert_eq!(CVPixelBufferCreate(ptr::null(),320,180,u32::from_be_bytes(if full { *b"420f" } else { *b"420v" }),ptr::null(),&mut raw),0);
        assert!(!raw.is_null());
        assert_eq!(CVPixelBufferLockBaseAddress(raw,0),0);
        for (plane,rows,value) in [(0,180,if full { 0 } else { 16 }),(1,90,128)] {
            let base = CVPixelBufferGetBaseAddressOfPlane(raw,plane);
            let stride = CVPixelBufferGetBytesPerRowOfPlane(raw,plane);
            assert!(!base.is_null());
            for row in 0..rows { ptr::write_bytes(base.add(row*stride),value,320); }
        }
        assert_eq!(CVPixelBufferUnlockBaseAddress(raw,0),0);
        if let Some(transfer) = transfer {
            CVBufferSetAttachment(raw,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_709_2,1);
            CVBufferSetAttachment(raw,kCVImageBufferTransferFunctionKey,transfer,1);
            CVBufferSetAttachment(raw,kCVImageBufferYCbCrMatrixKey,kCVImageBufferYCbCrMatrix_ITU_R_709_2,1);
        }
        let retained = PixelBuffer::from_raw(raw).unwrap();
        CFRelease(raw);
        retained
    }
}
fn encoder() -> VideoEncoder {
    VideoEncoder::new(EncoderConfig { codec: VideoCodec::H264, width: 320, height: 180, fps: 30, bitrate_kbps: 500, preference: EncoderPreference::Software },|| {}).unwrap()
}
fn submit(encoder: &mut VideoEncoder, image: &PixelBuffer, pts: i64) {
    encoder.encode(image,pts,true).unwrap();
    assert!(encoder.wait_output(Duration::from_secs(2)).unwrap().is_some());
}

#[test]
fn color_configuration_preserves_stable_source_and_closes_on_transfer_change() {
    // SAFETY: these linked SDK constants are immortal CFStringRefs.
    let (bt709,srgb) = unsafe { (kCVImageBufferTransferFunction_ITU_R_709_2,kCVImageBufferTransferFunction_sRGB) };
    let first = image(false,Some(bt709));
    let same = image(false,Some(bt709));
    let changed = image(false,Some(srgb));
    let mut encoder = encoder();
    submit(&mut encoder,&first,0);
    submit(&mut encoder,&same,33333);
    assert!(matches!(encoder.encode(&changed,66666,true),Err(MediaError::InvalidFrame)));
    assert!(matches!(encoder.encode(&first,99999,true),Err(MediaError::Closed)));
}

#[test]
fn color_configuration_closes_on_missing_or_added_source_tag() {
    // SAFETY: this linked SDK constant is an immortal CFStringRef.
    let tagged = image(false,Some(unsafe { kCVImageBufferTransferFunction_ITU_R_709_2 }));
    let absent = image(false,None);
    for (first,changed) in [(&tagged,&absent),(&absent,&tagged)] {
        let mut encoder = encoder();
        submit(&mut encoder,first,0);
        assert!(matches!(encoder.encode(changed,33333,true),Err(MediaError::InvalidFrame)));
        assert!(matches!(encoder.encode(first,66666,true),Err(MediaError::Closed)));
    }
}

#[test]
fn color_configuration_closes_on_nv12_range_change() {
    let video = image(false,None);
    let full = image(true,None);
    let mut encoder = encoder();
    submit(&mut encoder,&video,0);
    assert!(matches!(encoder.encode(&full,33333,true),Err(MediaError::InvalidFrame)));
    assert!(matches!(encoder.encode(&video,66666,true),Err(MediaError::Closed)));
}

#[test]
fn color_configuration_rejects_non_string_attachment() {
    // SAFETY: the fixture owns this CFNumber through its attachment call, which
    // retains it. No pixel or attachment is mutated after from_raw.
    let malformed = unsafe {
        let value = 1i64;
        let number = CFNumberCreate(ptr::null(),4,(&value as *const i64).cast());
        assert!(!number.is_null());
        let malformed = image(false,Some(number));
        CFRelease(number);
        malformed
    };
    let mut encoder = encoder();
    assert!(matches!(encoder.encode(&malformed,0,true),Err(MediaError::InvalidFrame)));
    assert!(matches!(encoder.encode(&malformed,33333,true),Err(MediaError::Closed)));
}
