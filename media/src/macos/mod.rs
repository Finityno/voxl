//! VideoToolbox H.264/HEVC from immutable NV12 capture buffers. Hardware is tried
//! first, software is selectable and hardware selection is verified. No raw
//! pixels are copied into a CPU color-conversion buffer on the encode path.

mod ffi;
use ffi::*;
use crate::{EncodedFrame, EncoderConfig, EncoderPreference, EncoderProfile, EncoderRateControl, EncoderTuning, MediaError, MAX_FRAME_BYTES, nal_units_to_annex_b};
use crate::codec::VideoCodec;
use objc2_core_media::{CMTime, CMTimeFlags};
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc::{self, Receiver, SyncSender}};

/// Retained immutable native image. No safe API exposes writable pixel memory.
pub struct PixelBuffer(NonNull<c_void>);
// SAFETY: only ownership of a retained immutable CVPixelBuffer crosses threads;
// CoreFoundation retain/release are thread-safe. The unsafe constructor enforces
// the image's immutability until its final native retain is released.
unsafe impl Send for PixelBuffer {}
unsafe impl Sync for PixelBuffer {}
impl PixelBuffer {
    /// Retain one borrowed CVPixelBuffer.
    ///
    /// # Safety
    /// `buffer` must be a live CVPixelBuffer. Its pixels must remain immutable
    /// until all its native references, including VideoToolbox's, are released.
    pub unsafe fn from_raw(buffer: *const c_void) -> Result<Self, MediaError> {
        let value = NonNull::new(buffer.cast_mut()).ok_or(MediaError::InvalidFrame)?;
        unsafe { CFRetain(buffer); }
        Ok(Self(value))
    }
}
impl Drop for PixelBuffer { fn drop(&mut self) { unsafe { CFRelease(self.0.as_ptr()); } } }

struct CallbackState {
    sender: SyncSender<EncodedFrame>,
    failed: AtomicBool,
    pending: AtomicUsize,
    // Embedders using a collecting allocator supply its synchronous idle hook.
    // Every foreign callback announces an idle point, including early failures.
    idle: fn(),
    codec: VideoCodec,
}

/// A single producer. Poll output promptly; a blocked consumer closes the
/// encoder rather than receiving encoded deltas whose reference was discarded.
pub struct VideoEncoder {
    session: NonNull<c_void>,
    callback: Box<CallbackState>,
    receiver: Receiver<EncodedFrame>,
    config: EncoderConfig,
    hardware: bool,
    last_pts: Option<i64>,
}
// SAFETY: ownership moves as a whole; mutating session calls require &mut self.
// VideoToolbox is thread-safe and callbacks access only atomics and a sender.
unsafe impl Send for VideoEncoder {}

impl VideoEncoder {
    pub fn new(config: EncoderConfig, idle: fn()) -> Result<Self, MediaError> {
        Self::new_tuned(config, EncoderTuning::default(), idle)
    }

    /// Explicit local tuning for measured experiments. Non-default requests
    /// require hardware and fail instead of substituting software or a mode.
    /// The actual SPS/profile and decoder interoperability still need probing.
    pub fn new_tuned(config: EncoderConfig, tuning: EncoderTuning, idle: fn()) -> Result<Self, MediaError> {
        tuning.validate(&config)?;
        match config.preference {
            EncoderPreference::PreferHardware => Self::create(config, true, tuning, idle)
                .or_else(|_| Self::create(config, false, tuning, idle)),
            EncoderPreference::RequireHardware => Self::create(config, true, tuning, idle),
            EncoderPreference::Software => Self::create(config, false, tuning, idle),
        }
    }

    fn create(config: EncoderConfig, hardware: bool, tuning: EncoderTuning, idle: fn()) -> Result<Self, MediaError> {
        let codec = match config.codec {
            VideoCodec::H264 => u32::from_be_bytes(*b"avc1"),
            VideoCodec::Hevc => u32::from_be_bytes(*b"hvc1"),
            VideoCodec::Av1 => return Err(MediaError::UnsupportedCodec),
        };
        let low_latency = if tuning.rate_control == EncoderRateControl::LowLatency {
            Some(optional_constant(c"kVTVideoEncoderSpecification_EnableLowLatencyRateControl").ok_or(MediaError::Configuration)?)
        } else { None };
        // SAFETY: this linked framework constant is an immortal CFStringRef.
        let profile = if config.codec == VideoCodec::Hevc { unsafe { kVTProfileLevel_HEVC_Main_AutoLevel } } else {
            match tuning.profile {
                EncoderProfile::Compatible => h264_profile(hardware),
                EncoderProfile::H264High => optional_constant(c"kVTProfileLevel_H264_High_AutoLevel").ok_or(MediaError::Configuration)?,
                EncoderProfile::H264ConstrainedHigh => optional_constant(c"kVTProfileLevel_H264_ConstrainedHigh_AutoLevel").ok_or(MediaError::Configuration)?,
            }
        };
        let (sender, receiver) = mpsc::sync_channel(2);
        let mut callback = Box::new(CallbackState { sender, failed: AtomicBool::new(false), pending: AtomicUsize::new(0), idle, codec: config.codec });
        let mut session = ptr::null_mut();
        // Specification borrows only immortal framework constants; callbacks
        // are intentionally null so the dictionary does not own those values.
        let specification = unsafe { CFDictionaryCreateMutable(ptr::null(), if low_latency.is_some() { 2 } else { 1 }, ptr::null(), ptr::null()) };
        if specification.is_null() { return Err(MediaError::Configuration); }
        unsafe {
            CFDictionarySetValue(specification, if hardware { kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder }
                else { kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder },
                if hardware { kCFBooleanTrue } else { kCFBooleanFalse });
            if let Some(key) = low_latency { CFDictionarySetValue(specification, key, kCFBooleanTrue); }
        }
        let code = unsafe { VTCompressionSessionCreate(ptr::null(), config.width as i32, config.height as i32,
            codec, specification, ptr::null(), ptr::null(), Some(output),
            (&mut *callback as *mut CallbackState).cast(), &mut session) };
        unsafe { CFRelease(specification); }
        check("create encoder",code)?;
        let session = NonNull::new(session).ok_or(MediaError::Configuration)?;
        let mut encoder = Self { session, callback, receiver, config, hardware: false, last_pts: None };
        unsafe {
            encoder.property("real time", kVTCompressionPropertyKey_RealTime, kCFBooleanTrue)?;
            encoder.property("frame reordering", kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse)?;
            encoder.property("profile", kVTCompressionPropertyKey_ProfileLevel, profile)?;
            encoder.number("bitrate", kVTCompressionPropertyKey_AverageBitRate, i64::from(config.bitrate_kbps) * 1000)?;
            encoder.number("frame rate", kVTCompressionPropertyKey_ExpectedFrameRate, i64::from(config.fps))?;
            // Low-latency rate control specifies an infinite GOP. Its caller
            // owns explicit recovery/periodic IDR requests; do not overwrite
            // that selection with the ordinary two-second interval.
            if tuning.rate_control == EncoderRateControl::RealTime {
                encoder.number("keyframe interval", kVTCompressionPropertyKey_MaxKeyFrameInterval, i64::from(config.fps) * 2)?;
            }
            check("prepare encoder",VTCompressionSessionPrepareToEncodeFrames(encoder.session.as_ptr()))?;
            let mut actual = ptr::null();
            let code = VTSessionCopyProperty(encoder.session.as_ptr(), kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder, ptr::null(), &mut actual);
            if code == kVTPropertyNotSupportedErr && !hardware {
                // EnableHardwareAcceleratedVideoEncoder=false guarantees that
                // hardware will never be used. Software sessions need not
                // expose UsingHardwareAcceleratedVideoEncoder; Apple documents
                // querying it after enabling hardware. Other errors still fail.
                encoder.hardware = false;
            } else {
                check("query hardware encoder",code)?;
                if actual.is_null() { return Err(MediaError::Configuration); }
                encoder.hardware = CFBooleanGetValue(actual);
                CFRelease(actual);
            }
        }
        if hardware != encoder.hardware { return Err(MediaError::Configuration); }
        Ok(encoder)
    }

    fn property(&mut self, operation: &'static str, key: Object, value: Object) -> Result<(), MediaError> {
        check(operation,unsafe { VTSessionSetProperty(self.session.as_ptr(), key, value) })
    }
    fn number(&mut self, operation: &'static str, key: Object, value: i64) -> Result<(), MediaError> {
        let number = unsafe { CFNumberCreate(ptr::null(), 4, (&value as *const i64).cast()) };
        if number.is_null() { return Err(MediaError::Configuration); }
        let result = self.property(operation, key, number);
        unsafe { CFRelease(number); }
        result
    }
    pub fn using_hardware(&self) -> bool { self.hardware }

    pub fn encode(&mut self, image: &PixelBuffer, pts_micros: i64, force_keyframe: bool) -> Result<(), MediaError> {
        if self.callback.failed.load(Ordering::Acquire) { return Err(MediaError::Closed); }
        if pts_micros < 0 || self.last_pts.is_some_and(|last| pts_micros <= last) { return Err(MediaError::InvalidFrame); }
        let buffer = image.0.as_ptr();
        // Both NV12 video range and full range are accepted. VideoToolbox keeps
        // the buffer retained for as long as the asynchronous encoder needs it.
        if unsafe { CVPixelBufferGetWidth(buffer) } != self.config.width as usize
            || unsafe { CVPixelBufferGetHeight(buffer) } != self.config.height as usize
            || !matches!(unsafe { CVPixelBufferGetPixelFormatType(buffer) }, 0x34323076 | 0x34323066)
        { return Err(MediaError::InvalidFrame); }
        if self.callback.pending.fetch_add(1, Ordering::AcqRel) >= 3 {
            self.callback.pending.fetch_sub(1, Ordering::AcqRel);
            self.callback.failed.store(true, Ordering::Release);
            return Err(MediaError::Backpressure);
        }
        let properties = if force_keyframe { unsafe {
            let dict = CFDictionaryCreateMutable(ptr::null(), 1, ptr::null(), ptr::null());
            if !dict.is_null() { CFDictionarySetValue(dict, kVTEncodeFrameOptionKey_ForceKeyFrame, kCFBooleanTrue); }
            dict
        } } else { ptr::null_mut() };
        if force_keyframe && properties.is_null() {
            self.callback.pending.fetch_sub(1, Ordering::AcqRel);
            return Err(MediaError::Configuration);
        }
        let result = unsafe { VTCompressionSessionEncodeFrame(self.session.as_ptr(), buffer,
            CMTime { value: pts_micros, timescale: 1_000_000, flags: CMTimeFlags::Valid, epoch: 0 },
            CMTime { value: 1, timescale: self.config.fps as i32, flags: CMTimeFlags::Valid, epoch: 0 },
            properties, ptr::null_mut(), ptr::null_mut()) };
        if !properties.is_null() { unsafe { CFRelease(properties); } }
        if result != 0 {
            // An immediate failure has no output callback. Close permanently;
            // do not continue a dependency chain after a failed encode.
            self.callback.failed.store(true, Ordering::Release);
            return Err(MediaError::Platform { operation: "encode frame", code: result });
        }
        self.last_pts = Some(pts_micros);
        Ok(())
    }

    pub fn take_output(&self) -> Result<Option<EncodedFrame>, MediaError> {
        if self.callback.failed.load(Ordering::Acquire) { return Err(MediaError::Closed); }
        match self.receiver.try_recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(MediaError::Closed),
        }
    }

    /// Block only a dedicated native worker until output or the supplied bound.
    /// This avoids a one-millisecond polling loop between native callbacks.
    pub fn wait_output(&self, timeout: std::time::Duration) -> Result<Option<EncodedFrame>, MediaError> {
        if self.callback.failed.load(Ordering::Acquire) { return Err(MediaError::Closed); }
        let result = match self.receiver.recv_timeout(timeout) {
            Ok(frame) => Ok(Some(frame)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(MediaError::Closed),
        };
        if self.callback.failed.load(Ordering::Acquire) { return Err(MediaError::Closed); }
        result
    }

}
impl Drop for VideoEncoder {
    fn drop(&mut self) {
        // Invalidation tears down the codec and completes its outstanding
        // callbacks before the context is destroyed. Do not run this on GPUI.
        unsafe { VTCompressionSessionInvalidate(self.session.as_ptr()); CFRelease(self.session.as_ptr()); }
    }
}
fn check(operation: &'static str, code: i32) -> Result<(), MediaError> { if code == 0 { Ok(()) } else { Err(MediaError::Platform { operation, code }) } }

fn optional_constant(name: &std::ffi::CStr) -> Option<Object> {
    // SAFETY: the linked framework is loaded; these named SDK data symbols
    // contain immortal CFStringRefs. A data symbol needs one indirection.
    unsafe {
        let symbol = dlsym(RTLD_DEFAULT,name.as_ptr());
        if symbol.is_null() { return None; }
        let value = *symbol.cast::<Object>();
        (!value.is_null()).then_some(value)
    }
}

fn h264_profile(hardware: bool) -> Object {
    // Constrained Baseline is the interoperable WebRTC profile. VideoToolbox's
    // ordinary hardware Baseline emits 4200, which WebKit's 42e0 capabilities
    // do not advertise. Request the actual constrained encoder profile instead
    // of rewriting SDP or claiming constraints absent from the native SPS.
    // The constant exists from macOS 12; older standalone users retain the
    // previous Baseline path and its truthful actual-profile reporting.
    // SAFETY: VideoToolbox is already linked/loaded. This optional SDK data
    // symbol is a process-lifetime CFStringRef, hence one pointer indirection.
    unsafe {
        // Apple's software encoder rejects the constrained-profile setting.
        // Its supported Baseline setting emits constrained SPS flags; report
        // that actual profile rather than assuming the property was accepted.
        if !hardware { return kVTProfileLevel_H264_Baseline_AutoLevel; }
        let symbol = dlsym(RTLD_DEFAULT,c"kVTProfileLevel_H264_ConstrainedBaseline_AutoLevel".as_ptr());
        if symbol.is_null() { kVTProfileLevel_H264_Baseline_AutoLevel } else { *symbol.cast::<Object>() }
    }
}

unsafe extern "C" fn output(context: *mut c_void, _source: *mut c_void, status: i32, flags: u32, sample: Object) {
    // SAFETY: the boxed context is pinned through session invalidation. The
    // callback only reads synchronized fields, never the owning encoder.
    let state = unsafe { &*context.cast::<CallbackState>() };
    // Neither an embedder hook nor a Rust panic may unwind across Apple's C
    // callback boundary. Run the idle hook even when parsing fails.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        output_inner(state, status, flags, sample);
    }));
    let idle = std::panic::catch_unwind(state.idle);
    if result.is_err() || idle.is_err() { state.failed.store(true, Ordering::Release); }
}

fn output_inner(state: &CallbackState, status: i32, flags: u32, sample: Object) {
    state.pending.fetch_sub(1, Ordering::AcqRel);
    if status != 0 || flags & 2 != 0 || sample.is_null() {
        state.failed.store(true, Ordering::Release);
        return;
    }
    let parsed = unsafe { read_sample(sample,state.codec) };
    match parsed {
        Ok(frame) if !state.failed.load(Ordering::Acquire) => {
            if state.sender.try_send(frame).is_err() { state.failed.store(true, Ordering::Release); }
        }
        _ => { state.failed.store(true, Ordering::Release); }
    }
}

unsafe fn read_sample(sample: Object, codec: VideoCodec) -> Result<EncodedFrame, MediaError> {
    let data = unsafe { CMSampleBufferGetDataBuffer(sample) };
    let format = unsafe { CMSampleBufferGetFormatDescription(sample) };
    if data.is_null() || format.is_null() { return Err(MediaError::InvalidFrame); }
    let length = unsafe { CMBlockBufferGetDataLength(data) };
    if length == 0 || length > MAX_FRAME_BYTES { return Err(MediaError::InvalidFrame); }
    let parameter_set = match codec {
        VideoCodec::H264 => CMVideoFormatDescriptionGetH264ParameterSetAtIndex,
        VideoCodec::Hevc => CMVideoFormatDescriptionGetHEVCParameterSetAtIndex,
        VideoCodec::Av1 => return Err(MediaError::UnsupportedCodec),
    };
    let mut count = 0;
    let mut prefix = 0;
    check("read video parameter sets",unsafe { parameter_set(format,0,ptr::null_mut(),ptr::null_mut(),&mut count,&mut prefix) })?;
    if count > 8 { return Err(MediaError::InvalidFrame); }
    let mut avcc = vec![0; length];
    check("read encoded bytes",unsafe { CMBlockBufferCopyDataBytes(data,0,length,avcc.as_mut_ptr().cast()) })?;
    let (mut annex_b, keyframe) = nal_units_to_annex_b(&avcc, prefix as usize, codec)?;
    if keyframe {
        let mut header = Vec::new();
        for index in 0..count {
            let mut bytes = ptr::null();
            let mut length = 0;
            check("read video parameter sets",unsafe { parameter_set(format,index,&mut bytes,&mut length,ptr::null_mut(),ptr::null_mut()) })?;
            if bytes.is_null() || length == 0 || length > 64 * 1024 { return Err(MediaError::InvalidFrame); }
            header.extend_from_slice(&[0,0,0,1]);
            header.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes,length) });
        }
        if header.len().saturating_add(annex_b.len()) > MAX_FRAME_BYTES { return Err(MediaError::InvalidFrame); }
        header.append(&mut annex_b);
        annex_b = header;
    }
    let pts = unsafe { CMSampleBufferGetPresentationTimeStamp(sample) };
    if pts.timescale <= 0 { return Err(MediaError::InvalidFrame); }
    let micros = i128::from(pts.value) * 1_000_000 / i128::from(pts.timescale);
    let presentation_time_micros = i64::try_from(micros).map_err(|_| MediaError::InvalidFrame)?;
    Ok(EncodedFrame { codec, annex_b, keyframe, presentation_time_micros })
}
