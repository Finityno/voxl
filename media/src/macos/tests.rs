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

fn callback_state()->(CallbackState,Receiver<Result<EncodedFrame,MediaError>>) {
    let(sender,receiver)=mpsc::sync_channel(2);
    (CallbackState {sender,failure:OnceLock::new(),pending:AtomicUsize::new(1),idle:|| {},codec:VideoCodec::H264,output_wake:OnceLock::new()},receiver)
}

struct CountWake {count:AtomicUsize,panic:bool}
impl OutputWake for CountWake {
    fn wake(&self) {self.count.fetch_add(1,Ordering::AcqRel);assert!(!self.panic,"injected wake failure");}
}
#[test]
fn output_wake_publishes_before_signaling_and_signals_only_the_first_failure() {
    let(state,receiver)=callback_state();let wake=Arc::new(CountWake {count:AtomicUsize::new(0),panic:false});
    assert!(state.output_wake.set(wake.clone()).is_ok());
    state.publish(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65],keyframe:true,presentation_time_micros:0});
    assert_eq!(wake.count.load(Ordering::Acquire),1);assert!(receiver.try_recv().unwrap().is_ok());
    state.fail(MediaError::Backpressure);state.fail(MediaError::Closed);
    assert_eq!(wake.count.load(Ordering::Acquire),2);assert_eq!(receiver.try_recv().unwrap().unwrap_err(),MediaError::Backpressure);
}
#[test]
fn output_wake_panic_is_contained_and_preserves_the_first_native_failure() {
    for failed in [false,true] {
        let(state,receiver)=callback_state();let wake=Arc::new(CountWake {count:AtomicUsize::new(0),panic:true});
        assert!(state.output_wake.set(wake.clone()).is_ok());
        if failed {state.fail(MediaError::InvalidFrame);} else {
            state.publish(EncodedFrame {codec:VideoCodec::H264,annex_b:vec![1],keyframe:true,presentation_time_micros:0});
        }
        assert_eq!(wake.count.load(Ordering::Acquire),1);
        assert_eq!(state.failure.get(),Some(&if failed {MediaError::InvalidFrame} else {MediaError::Closed}));
        assert!(receiver.try_recv().is_ok());
    }
}
#[test]
fn output_wake_installation_is_once_only_and_precedes_native_submission() {
    let wake=Arc::new(CountWake {count:AtomicUsize::new(0),panic:false});let mut encoder=encoder();
    assert!(encoder.install_output_wake(wake.clone()).is_ok());
    assert_eq!(encoder.install_output_wake(wake.clone()),Err(MediaError::Configuration));
    submit(&mut encoder,&image(false,None),0);
    assert_eq!(encoder.install_output_wake(wake.clone()),Err(MediaError::Configuration));
    drop(encoder);assert_eq!(wake.count.load(Ordering::Acquire),1);
}

#[test]
fn callback_failures_preserve_first_cause_and_wake_the_output_receiver() {
    for(status,flags,expected)in [
        (-12909,0,MediaError::Platform {operation:"encoder output callback",code:-12909}),
        (0,2,MediaError::Platform {operation:"encoder dropped output",code:2}),
        (0,0,MediaError::InvalidFrame),
    ] {
        let(state,receiver)=callback_state();
        output_inner(&state,status,flags,ptr::null());
        assert_eq!(state.pending.load(Ordering::Acquire),0);
        assert_eq!(state.failure.get(),Some(&expected));
        assert_eq!(receiver.recv_timeout(Duration::from_millis(20)).unwrap().unwrap_err(),expected);
        state.fail(MediaError::Closed);
        assert_eq!(state.failure.get(),Some(&expected));
        assert!(matches!(receiver.try_recv(),Err(mpsc::TryRecvError::Empty)));
    }
}

#[test]
fn callback_full_queue_is_terminal_without_discarding_the_first_failure() {
    let(state,receiver)=callback_state();
    let frame=||EncodedFrame {codec:VideoCodec::H264,annex_b:vec![0,0,0,1,0x65],keyframe:true,presentation_time_micros:0};
    state.publish(frame());state.publish(frame());state.publish(frame());
    assert_eq!(state.failure.get(),Some(&MediaError::Backpressure));
    state.fail(MediaError::Closed);state.publish(frame());
    assert_eq!(state.failure.get(),Some(&MediaError::Backpressure));
    assert!(receiver.try_recv().unwrap().is_ok());assert!(receiver.try_recv().unwrap().is_ok());
    assert!(matches!(receiver.try_recv(),Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn callback_idle_panic_cannot_escape_or_overwrite_a_native_failure() {
    let(mut state,receiver)=callback_state();state.idle=||panic!("injected idle failure");
    // SAFETY: this stack-owned callback context stays live for this synchronous
    // invocation. A nonzero status prevents inspecting the null sample.
    unsafe {output((&mut state as *mut CallbackState).cast(),ptr::null_mut(),-12909,0,ptr::null());}
    let expected=MediaError::Platform {operation:"encoder output callback",code:-12909};
    assert_eq!(state.failure.get(),Some(&expected));
    assert_eq!(receiver.recv_timeout(Duration::from_millis(20)).unwrap().unwrap_err(),expected);
}

#[test]
fn encoding_priority_requires_hardware_without_constructing_a_native_session() {
    let mut config=EncoderConfig {codec:VideoCodec::H264,width:320,height:180,fps:30,bitrate_kbps:500,preference:EncoderPreference::Software};
    for preference in [EncoderPreference::Software,EncoderPreference::PreferHardware] {
        config.preference=preference;
        assert!(EncodingPriority::Default.validate(&config).is_ok());
        for priority in [EncodingPriority::Speed,EncodingPriority::Quality] {
            assert!(matches!(VideoEncoder::new_tuned_with_priority(config,EncoderTuning::default(),priority,|| {}),Err(MediaError::Configuration)));
        }
    }
    config.preference=EncoderPreference::RequireHardware;
    for priority in [EncodingPriority::Default,EncodingPriority::Speed,EncodingPriority::Quality] {assert!(priority.validate(&config).is_ok());}
}

#[test]
fn maximum_frame_qp_rejects_invalid_limits_and_implicit_software_fallback() {
    for value in [0,52,255] {assert_eq!(MaxFrameQp::new(value),Err(MediaError::Configuration));}
    let mut config=EncoderConfig {codec:VideoCodec::H264,width:320,height:180,fps:30,bitrate_kbps:500,preference:EncoderPreference::Software};
    for value in [1,28,51] {
        let limit=MaxFrameQp::new(value).unwrap();assert_eq!(limit.get(),value);
        let options=EncodingOptions {max_frame_qp:Some(limit),..Default::default()};
        for preference in [EncoderPreference::Software,EncoderPreference::PreferHardware] {
            config.preference=preference;
            assert!(matches!(VideoEncoder::new_tuned_with_options(config,EncoderTuning::default(),options,|| {}),Err(MediaError::Configuration)));
        }
        config.preference=EncoderPreference::RequireHardware;assert!(options.validate(&config).is_ok());
    }
}

#[test]
fn maximum_frame_qp_readback_rejects_noninteger_or_mismatched_native_values() {
    // SAFETY: these number inputs remain valid during creation; each returned
    // owned CF object is released after inspecting its type and exact value.
    unsafe {
        let requested=28i64;let fractional=28.5f64;
        let integer=CFNumberCreate(ptr::null(),4,(&requested as *const i64).cast());
        let real=CFNumberCreate(ptr::null(),6,(&fractional as *const f64).cast());
        assert!(!integer.is_null() && !real.is_null());
        assert!(integer_property_matches(integer,28));
        assert!(!integer_property_matches(integer,29));
        assert!(!integer_property_matches(real,28));
        assert!(!integer_property_matches(kCFBooleanTrue,1));
        assert!(!integer_property_matches(ptr::null(),28));
        CFRelease(integer);CFRelease(real);
    }
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
