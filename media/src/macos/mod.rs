//! VideoToolbox H.264/HEVC from immutable NV12 capture buffers. Hardware is tried
//! first, software is selectable and hardware selection is verified. No raw
//! pixels are copied into a CPU color-conversion buffer on the encode path.

mod ffi;
use ffi::*;
use crate::{EncodedFrame, EncoderAcceleration, EncoderConfig, EncoderPreference, EncoderProfile, EncoderRateControl, EncoderTuning, MediaError, MAX_FRAME_BYTES, nal_units_to_annex_b};
use crate::codec::VideoCodec;
use objc2_core_media::{CMTime, CMTimeFlags};
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::{Arc, OnceLock, atomic::{AtomicUsize, Ordering}, mpsc::{self, Receiver, SyncSender}};

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
    /// `buffer` must be a live CVPixelBuffer. Its pixels and attachments must
    /// remain immutable until all its native references, including
    /// VideoToolbox's, are released.
    pub unsafe fn from_raw(buffer: *const c_void) -> Result<Self, MediaError> {
        let value = NonNull::new(buffer.cast_mut()).ok_or(MediaError::InvalidFrame)?;
        unsafe { CFRetain(buffer); }
        Ok(Self(value))
    }
}
impl Drop for PixelBuffer { fn drop(&mut self) { unsafe { CFRelease(self.0.as_ptr()); } } }

// Colorimetry is established by the first source, never inferred from screen
// dimensions or replaced with fixed primaries. Keep just three copied strings,
// not a retained source frame or a CPU-converted pixel buffer.
struct SourceColor {
    pixel_format: u32,
    tags: [Object; 3],
}
impl SourceColor {
    fn keys() -> [Object; 3] {
        // SAFETY: these linked framework keys are immortal CFStringRefs.
        unsafe { [kCVImageBufferColorPrimariesKey,kCVImageBufferTransferFunctionKey,kCVImageBufferYCbCrMatrixKey] }
    }
    fn copy_from(buffer: Object) -> Result<Self, MediaError> {
        // SAFETY: the caller holds an immutable, validated PixelBuffer.
        let mut color = Self { pixel_format: unsafe { CVPixelBufferGetPixelFormatType(buffer) }, tags: [ptr::null();3] };
        for (index,key) in Self::keys().into_iter().enumerate() {
            // SAFETY: the retained source owns its borrowed attachment while
            // its type is checked and an immutable CFString copy is created.
            unsafe {
                let tag = CVBufferGetAttachment(buffer,key,ptr::null_mut());
                if tag.is_null() { continue; }
                if CFGetTypeID(tag) != CFStringGetTypeID() { return Err(MediaError::InvalidFrame); }
                color.tags[index] = CFStringCreateCopy(ptr::null(),tag);
                if color.tags[index].is_null() { return Err(MediaError::Configuration); }
            }
        }
        Ok(color)
    }
    fn matches(&self, buffer: Object) -> bool {
        // SAFETY: both the stored strings and the immutable source are live.
        unsafe {
            if self.pixel_format != CVPixelBufferGetPixelFormatType(buffer) { return false; }
            Self::keys().into_iter().zip(self.tags).all(|(key,expected)| {
                let actual = CVBufferGetAttachment(buffer,key,ptr::null_mut());
                if actual.is_null() || expected.is_null() { return actual == expected; }
                CFGetTypeID(actual) == CFStringGetTypeID() && CFEqual(actual,expected)
            })
        }
    }
}
impl Drop for SourceColor {
    fn drop(&mut self) {
        // SAFETY: each nonnull field owns a CFStringCreateCopy reference.
        for tag in self.tags { if !tag.is_null() { unsafe { CFRelease(tag); } } }
    }
}
enum ColorConfiguration { AwaitingFirstFrame, Established(SourceColor) }

/// Explicit local experiment, independent of profile and rate control.
/// Default leaves VideoToolbox's preference unspecified. Speed can sacrifice
/// quality at the same bitrate and requires independent decoded quality checks.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum EncodingPriority {#[default] Default,Speed,Quality}
impl EncodingPriority {
    fn validate(self,config:&EncoderConfig)->Result<(),MediaError> {
        if self!=Self::Default && config.preference!=EncoderPreference::RequireHardware {return Err(MediaError::Configuration);}
        Ok(())
    }
}

/// Explicit upper quantizer limit, not an observed quality guarantee. A tighter
/// limit can make the native rate controller drop frames; that remains a
/// terminal encoder failure. Availability depends on the encoder and its mode.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct MaxFrameQp(u8);
impl MaxFrameQp {
    pub fn new(value:u8)->Result<Self,MediaError> {
        if !(1..=51).contains(&value) {return Err(MediaError::Configuration);}
        Ok(Self(value))
    }
    pub fn get(self)->u8 {self.0}
}

/// Local opt-in experiments. Default leaves all native properties untouched.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct EncodingOptions {
    pub priority:EncodingPriority,
    pub max_frame_qp:Option<MaxFrameQp>,
    /// Maximum native compression-window delay, in frames. Zero may make the
    /// encode call synchronous. This neither changes our two-output bound nor
    /// proves callback latency or quality; unsupported limits fail at creation.
    pub max_frame_delay:Option<u32>,
}
impl EncodingOptions {
    fn validate(self,config:&EncoderConfig)->Result<(),MediaError> {
        self.priority.validate(config)?;
        if (self.max_frame_qp.is_some() || self.max_frame_delay.is_some()) && config.preference!=EncoderPreference::RequireHardware {return Err(MediaError::Configuration);}
        Ok(())
    }
}

/// Optional event wake for a worker that also waits for capture or delivery
/// capacity. Output is queued before this hook runs, including terminal errors.
/// The hook must return promptly and must not call back into this encoder.
/// Panics close the encoder and are contained at the Rust/C boundary.
pub trait OutputWake:Send+Sync {fn wake(&self);}

struct CallbackState {
    sender: SyncSender<Result<EncodedFrame,MediaError>>,
    failure: OnceLock<MediaError>,
    pending: AtomicUsize,
    // Embedders using a collecting allocator supply its synchronous idle hook.
    // Every foreign callback announces an idle point, including early failures.
    idle: fn(),
    codec: VideoCodec,
    output_wake: OnceLock<Arc<dyn OutputWake>>,
}
impl CallbackState {
    fn wake(&self) {
        if let Some(wake)=self.output_wake.get()
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(||wake.wake())).is_err()
            && self.failure.set(MediaError::Closed).is_ok() {
            let _=self.sender.try_send(Err(MediaError::Closed));
        }
    }
    fn fail(&self,error:MediaError) {
        if self.failure.set(error).is_ok() {
            // Wake a waiting worker on failure without blocking the native
            // callback. A full queue already has output to wake that worker;
            // its terminal-state check prevents publishing those deltas.
            let _=self.sender.try_send(Err(error));
            self.wake();
        }
    }
    fn publish(&self,frame:EncodedFrame) {
        if self.failure.get().is_some() {return;}
        match self.sender.try_send(Ok(frame)) {
            Ok(())=>self.wake(),
            Err(mpsc::TrySendError::Full(_))=>self.fail(MediaError::Backpressure),
            Err(mpsc::TrySendError::Disconnected(_))=>self.fail(MediaError::Closed),
        }
    }
}

/// A single producer. Poll output promptly; a blocked consumer closes the
/// encoder rather than receiving encoded deltas whose reference was discarded.
pub struct VideoEncoder {
    session: NonNull<c_void>,
    callback: Box<CallbackState>,
    receiver: Receiver<Result<EncodedFrame,MediaError>>,
    config: EncoderConfig,
    bitrate_target_kbps: u32,
    burst_limit: Option<(u32,u32)>,
    acceleration: EncoderAcceleration,
    options: EncodingOptions,
    last_pts: Option<i64>,
    color: ColorConfiguration,
}
// SAFETY: ownership moves as a whole; mutating session calls require &mut self.
// VideoToolbox is thread-safe; callbacks access synchronized failure/pending
// state and a sender, never the owning producer.
// Copied color tags are immutable CFStrings owned by this producer.
unsafe impl Send for VideoEncoder {}

impl VideoEncoder {
    pub fn new(config: EncoderConfig, idle: fn()) -> Result<Self, MediaError> {
        Self::new_tuned(config, EncoderTuning::default(), idle)
    }

    /// Explicit local tuning for measured experiments. Non-default requests
    /// require hardware and fail instead of substituting software or a mode.
    /// The actual SPS/profile and decoder interoperability still need probing.
    pub fn new_tuned(config: EncoderConfig, tuning: EncoderTuning, idle: fn()) -> Result<Self, MediaError> {
        Self::new_tuned_with_priority(config,tuning,EncodingPriority::Default,idle)
    }

    /// An explicit non-default priority requires hardware, successful property
    /// setting and matching typed readback after preparation. Unsupported or
    /// ignored requests fail rather than silently running a different mode.
    /// No default preference is queried or changed for existing constructors.
    pub fn new_tuned_with_priority(config: EncoderConfig, tuning: EncoderTuning, priority: EncodingPriority, idle: fn()) -> Result<Self, MediaError> {
        Self::new_tuned_with_options(config,tuning,EncodingOptions {priority,..Default::default()},idle)
    }

    /// Non-default options require hardware and matching native typed readback.
    /// Setter acceptance alone does not establish support, actual frame QP,
    /// speed, quality or profile admission. Dropped frames remain terminal.
    pub fn new_tuned_with_options(config: EncoderConfig, tuning: EncoderTuning, options: EncodingOptions, idle: fn()) -> Result<Self, MediaError> {
        tuning.validate(&config)?;
        options.validate(&config)?;
        match config.preference {
            EncoderPreference::PreferHardware => Self::create(config, true, tuning, options, idle)
                .or_else(|_| Self::create(config, false, tuning, options, idle)),
            EncoderPreference::RequireHardware => Self::create(config, true, tuning, options, idle),
            EncoderPreference::Software => Self::create(config, false, tuning, options, idle),
        }
    }

    fn create(config: EncoderConfig, hardware: bool, tuning: EncoderTuning, options: EncodingOptions, idle: fn()) -> Result<Self, MediaError> {
        let codec = match config.codec {
            VideoCodec::H264 => u32::from_be_bytes(*b"avc1"),
            VideoCodec::Hevc => u32::from_be_bytes(*b"hvc1"),
            VideoCodec::Av1 => return Err(MediaError::UnsupportedCodec),
        };
        let low_latency = if tuning.rate_control == EncoderRateControl::LowLatency {
            Some(optional_constant(c"kVTVideoEncoderSpecification_EnableLowLatencyRateControl").ok_or(MediaError::Configuration)?)
        } else { None };
        let priority=options.priority;
        let priority_key = if priority!=EncodingPriority::Default {
            Some(optional_constant(c"kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality").ok_or(MediaError::Configuration)?)
        } else {None};
        let qp_key=if options.max_frame_qp.is_some() {
            Some(optional_constant(c"kVTCompressionPropertyKey_MaxAllowedFrameQP").ok_or(MediaError::Configuration)?)
        } else {None};
        let delay_key=if options.max_frame_delay.is_some() {
            Some(optional_constant(c"kVTCompressionPropertyKey_MaxFrameDelayCount").ok_or(MediaError::Configuration)?)
        } else {None};
        // SAFETY: this linked framework constant is an immortal CFStringRef.
        let profile = if config.codec == VideoCodec::Hevc { unsafe { kVTProfileLevel_HEVC_Main_AutoLevel } } else {
            match tuning.profile {
                EncoderProfile::Compatible => h264_profile(hardware),
                EncoderProfile::H264High => optional_constant(c"kVTProfileLevel_H264_High_AutoLevel").ok_or(MediaError::Configuration)?,
                EncoderProfile::H264ConstrainedHigh => optional_constant(c"kVTProfileLevel_H264_ConstrainedHigh_AutoLevel").ok_or(MediaError::Configuration)?,
            }
        };
        let (sender, receiver) = mpsc::sync_channel(2);
        let mut callback = Box::new(CallbackState { sender, failure: OnceLock::new(), pending: AtomicUsize::new(0), idle, codec: config.codec, output_wake: OnceLock::new() });
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
        let mut encoder = Self { session, callback, receiver, config, bitrate_target_kbps:config.bitrate_kbps,burst_limit:None,
            acceleration: if hardware { EncoderAcceleration::RequiredHardware } else { EncoderAcceleration::DisabledHardware }, options, last_pts: None, color: ColorConfiguration::AwaitingFirstFrame };
        unsafe {
            encoder.property("real time", kVTCompressionPropertyKey_RealTime, kCFBooleanTrue)?;
            encoder.property("frame reordering", kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse)?;
            encoder.property("profile", kVTCompressionPropertyKey_ProfileLevel, profile)?;
            encoder.number("bitrate", kVTCompressionPropertyKey_AverageBitRate, i64::from(config.bitrate_kbps) * 1000)?;
            encoder.number("frame rate", kVTCompressionPropertyKey_ExpectedFrameRate, i64::from(config.fps))?;
            if let Some(key)=priority_key {
                encoder.property("encoding priority",key,if priority==EncodingPriority::Speed {kCFBooleanTrue} else {kCFBooleanFalse})?;
            }
            if let (Some(key),Some(limit))=(qp_key,options.max_frame_qp) {
                encoder.number("maximum frame QP",key,i64::from(limit.get()))?;
            }
            if let (Some(key),Some(limit))=(delay_key,options.max_frame_delay) {
                encoder.number("maximum frame delay",key,i64::from(limit))?;
            }
            // Low-latency rate control specifies an infinite GOP. Its caller
            // owns explicit recovery/periodic IDR requests; do not overwrite
            // that selection with the ordinary two-second interval.
            if tuning.rate_control == EncoderRateControl::RealTime {
                encoder.number("keyframe interval", kVTCompressionPropertyKey_MaxKeyFrameInterval, i64::from(config.fps) * 2)?;
            }
            check("prepare encoder",VTCompressionSessionPrepareToEncodeFrames(encoder.session.as_ptr()))?;
            if let Some(key)=priority_key {
                let mut actual=ptr::null();
                check("query encoding priority",VTSessionCopyProperty(encoder.session.as_ptr(),key,ptr::null(),&mut actual))?;
                if actual.is_null() {return Err(MediaError::Configuration);}
                let matches=CFGetTypeID(actual)==CFBooleanGetTypeID() && CFBooleanGetValue(actual)==(priority==EncodingPriority::Speed);
                CFRelease(actual);
                if !matches {return Err(MediaError::Configuration);}
            }
            if let (Some(key),Some(limit))=(qp_key,options.max_frame_qp) {
                let mut actual=ptr::null();
                check("query maximum frame QP",VTSessionCopyProperty(encoder.session.as_ptr(),key,ptr::null(),&mut actual))?;
                if actual.is_null() {return Err(MediaError::Configuration);}
                let matches=integer_property_matches(actual,i64::from(limit.get()));
                CFRelease(actual);
                if !matches {return Err(MediaError::Configuration);}
            }
            if let (Some(key),Some(limit))=(delay_key,options.max_frame_delay) {
                let mut actual=ptr::null();
                check("query maximum frame delay",VTSessionCopyProperty(encoder.session.as_ptr(),key,ptr::null(),&mut actual))?;
                if actual.is_null() {return Err(MediaError::Configuration);}
                let matches=integer_property_matches(actual,i64::from(limit));
                CFRelease(actual);
                if !matches {return Err(MediaError::Configuration);}
            }
            let mut actual = ptr::null();
            let code = VTSessionCopyProperty(encoder.session.as_ptr(), kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder, ptr::null(), &mut actual);
            if code == kVTPropertyNotSupportedErr {
                // Explicitly disabled hardware and hardware-required low-delay
                // sessions can omit this property. Creation's specification
                // still guarantees the selection. Keep that observation source
                // distinct and retain strict status checks for ordinary HW.
                encoder.acceleration = EncoderAcceleration::without_status(hardware,config.preference,tuning)
                    .ok_or(MediaError::Platform { operation: "query hardware encoder", code })?;
            } else {
                check("query hardware encoder",code)?;
                if actual.is_null() { return Err(MediaError::Configuration); }
                encoder.acceleration = if CFBooleanGetValue(actual) { EncoderAcceleration::QueriedHardware } else { EncoderAcceleration::QueriedSoftware };
                CFRelease(actual);
            }
        }
        if hardware != encoder.using_hardware() { return Err(MediaError::Configuration); }
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
    pub fn using_hardware(&self) -> bool { self.acceleration.using_hardware() }
    pub fn acceleration_selection(&self) -> EncoderAcceleration { self.acceleration }
    /// Initially the configured request, not a native observation. A successful
    /// set_bitrate_target verifies the updated average target with typed readback.
    /// Neither value is a hard wire-rate limit or an observed encoded bitrate.
    pub fn bitrate_target_kbps(&self)->u32 {self.bitrate_target_kbps}
    /// Verified hard encoded-byte limit and native window in milliseconds.
    /// The CPU window reduces its sustained rate below the average request.
    pub fn burst_limit(&self)->Option<(u32,u32)> {self.burst_limit}

    /// Retune future submissions on this same worker-owned session, including
    /// the explicit CPU encoder. Already submitted frames keep their old target.
    /// Never exceed the original configured ceiling. A partially applied native
    /// change is terminal unless exact integer readback establishes its result.
    pub fn set_bitrate_target(&mut self,kbps:u32)->Result<(),MediaError> {
        if !(250..=self.config.bitrate_kbps).contains(&kbps) {return Err(MediaError::Configuration);}
        if self.callback.failure.get().is_some() {return Err(MediaError::Closed);}
        let result=(|| {
            let expected=i64::from(kbps)*1000;
            // SAFETY: this linked property is an immortal framework CFString.
            let key=unsafe {kVTCompressionPropertyKey_AverageBitRate};
            self.number("update bitrate",key,expected)?;
            let mut value=ptr::null();
            // SAFETY: the producer owns this live session. Copy returns an
            // owned CF object, inspected by type before integer conversion.
            let code=unsafe {VTSessionCopyProperty(self.session.as_ptr(),key,ptr::null(),&mut value)};
            let matches=code==0 && integer_property_matches(value,expected);
            if !value.is_null() {
                // SAFETY: Copy returned this owned CF object, even on failure.
                unsafe {CFRelease(value);}
            }
            check("read updated bitrate",code)?;
            if !matches {return Err(MediaError::Configuration);}
            Ok(())
        })();
        match result {
            Ok(())=>{self.bitrate_target_kbps=kbps;Ok(())},
            Err(error)=>{self.callback.fail(error);Err(error)},
        }
    }

    /// Adaptive qualification: cap bytes to 400 ms worth of the average target.
    /// The hardware window is 400 ms; CPU H.264 requires whole seconds and uses
    /// a conservative one-second window, lowering its sustained hard ceiling.
    /// This bounds keyframe bursts before
    /// pacing; it is not quality, achieved wire-rate or congestion admission.
    /// Unsupported/ignored properties or partial changes are terminal.
    pub fn set_streaming_bitrate_target(&mut self,kbps:u32)->Result<(),MediaError> {
        if !(250..=self.config.bitrate_kbps).contains(&kbps) {return Err(MediaError::Configuration);}
        if self.callback.failure.get().is_some() {return Err(MediaError::Closed);}
        let bytes=i64::from(kbps)*50;
        let seconds=if self.using_hardware() {0.4f64} else {1.0f64};
        let result=(|| {
            // SAFETY: stack numbers stay live through creation; the CFType
            // callbacks retain both numbers for any retained native array.
            let limit=unsafe {CFNumberCreate(ptr::null(),4,(&bytes as *const i64).cast())};
            if limit.is_null() {return Err(MediaError::Configuration);}
            let duration=unsafe {CFNumberCreate(ptr::null(),6,(&seconds as *const f64).cast())};
            if duration.is_null() {unsafe {CFRelease(limit);}return Err(MediaError::Configuration);}
            let values=[limit,duration];
            let array=unsafe {CFArrayCreate(ptr::null(),values.as_ptr(),2,&raw const kCFTypeArrayCallBacks)};
            unsafe {CFRelease(limit);CFRelease(duration);}
            if array.is_null() {return Err(MediaError::Configuration);}
            // SAFETY: this linked property is an immortal framework CFString.
            let key=unsafe {kVTCompressionPropertyKey_DataRateLimits};
            let set=self.property("update burst limit",key,array);
            unsafe {CFRelease(array);}set?;
            let mut value=ptr::null();
            // SAFETY: this worker owns the live session; Copy returns a CF
            // object which is type-checked before array/number access.
            let code=unsafe {VTSessionCopyProperty(self.session.as_ptr(),key,ptr::null(),&mut value)};
            let matches=code==0 && burst_property_matches(value,bytes,seconds);
            if !value.is_null() {unsafe {CFRelease(value);}}
            check("read updated burst limit",code)?;
            if !matches {return Err(MediaError::Configuration);}
            self.set_bitrate_target(kbps)
        })();
        match &result {Ok(())=>self.burst_limit=Some((bytes as u32,if self.using_hardware() {400} else {1000})),Err(error)=>self.callback.fail(*error)}
        result
    }
    /// Non-default values had matching native CFBoolean readback. Default is
    /// unspecified, not an observation of the encoder's internal preference.
    pub fn encoding_priority(&self)->EncodingPriority {self.options.priority}
    /// A present request had matching native integer readback. None leaves the
    /// property untouched and observes no internal default or actual frame QP.
    pub fn max_frame_qp(&self)->Option<MaxFrameQp> {self.options.max_frame_qp}
    /// A present request had matching native integer readback. None observes
    /// no internal default. Zero can block encode until its output is emitted.
    pub fn max_frame_delay(&self)->Option<u32> {self.options.max_frame_delay}

    /// Install once, before the first successful submission. Existing callers
    /// keep their channel wait and no additional callback. A wake is a hint:
    /// recheck output/error and the other worker conditions under its wait lock.
    pub fn install_output_wake(&mut self,wake:Arc<dyn OutputWake>)->Result<(),MediaError> {
        if self.last_pts.is_some() || self.callback.pending.load(Ordering::Acquire)!=0 {return Err(MediaError::Configuration);}
        self.callback.output_wake.set(wake).map_err(|_|MediaError::Configuration)
    }

    fn configure_source_color(&mut self, buffer: Object) -> Result<(), MediaError> {
        match &self.color {
            ColorConfiguration::Established(color) => {
                if color.matches(buffer) { Ok(()) } else { Err(MediaError::InvalidFrame) }
            }
            ColorConfiguration::AwaitingFirstFrame => {
                let color = SourceColor::copy_from(buffer)?;
                // SAFETY: keys are immortal, values are retained CFStrings.
                // Do not set absent source properties or silently ignore a
                // codec that rejects the actual source's colorimetry.
                let keys = unsafe { [kVTCompressionPropertyKey_ColorPrimaries,kVTCompressionPropertyKey_TransferFunction,kVTCompressionPropertyKey_YCbCrMatrix] };
                for ((key,value),operation) in keys.into_iter().zip(color.tags)
                    .zip(["source color primaries","source transfer function","source YCbCr matrix"])
                { if !value.is_null() { self.property(operation,key,value)?; } }
                self.color = ColorConfiguration::Established(color);
                Ok(())
            }
        }
    }

    /// The first frame establishes source color tags and NV12 range. A later
    /// change closes this encoder; the caller must create a new generation.
    pub fn encode(&mut self, image: &PixelBuffer, pts_micros: i64, force_keyframe: bool) -> Result<(), MediaError> {
        if self.callback.failure.get().is_some() { return Err(MediaError::Closed); }
        if pts_micros < 0 || self.last_pts.is_some_and(|last| pts_micros <= last) { return Err(MediaError::InvalidFrame); }
        let buffer = image.0.as_ptr();
        // Both NV12 video range and full range are accepted. VideoToolbox keeps
        // the buffer retained for as long as the asynchronous encoder needs it.
        if unsafe { CVPixelBufferGetWidth(buffer) } != self.config.width as usize
            || unsafe { CVPixelBufferGetHeight(buffer) } != self.config.height as usize
            || !matches!(unsafe { CVPixelBufferGetPixelFormatType(buffer) }, 0x34323076 | 0x34323066)
        { return Err(MediaError::InvalidFrame); }
        if let Err(error) = self.configure_source_color(buffer) {
            self.callback.fail(error);
            return Err(error);
        }
        if self.callback.pending.fetch_add(1, Ordering::AcqRel) >= 3 {
            self.callback.pending.fetch_sub(1, Ordering::AcqRel);
            self.callback.fail(MediaError::Backpressure);
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
            self.callback.fail(MediaError::Platform {operation:"encode frame",code:result});
            return Err(MediaError::Platform { operation: "encode frame", code: result });
        }
        self.last_pts = Some(pts_micros);
        Ok(())
    }

    pub fn take_output(&self) -> Result<Option<EncodedFrame>, MediaError> {
        if self.callback.failure.get().is_some() { return Err(MediaError::Closed); }
        let result=match self.receiver.try_recv() {
            Ok(Ok(frame)) => Ok(Some(frame)),
            Ok(Err(_)) => Err(MediaError::Closed),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(MediaError::Closed),
        };
        if self.callback.failure.get().is_some() {return Err(MediaError::Closed);}
        result
    }

    /// First terminal cause, retained without replacing it with a later close.
    /// Later output/encode calls keep their existing permanent Closed contract.
    pub fn failure_cause(&self)->Option<MediaError> {self.callback.failure.get().copied()}

    /// Block only a dedicated native worker until output or the supplied bound.
    /// This avoids a one-millisecond polling loop between native callbacks.
    pub fn wait_output(&self, timeout: std::time::Duration) -> Result<Option<EncodedFrame>, MediaError> {
        if self.callback.failure.get().is_some() { return Err(MediaError::Closed); }
        let result = match self.receiver.recv_timeout(timeout) {
            Ok(Ok(frame)) => Ok(Some(frame)),
            Ok(Err(_)) => Err(MediaError::Closed),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(MediaError::Closed),
        };
        if self.callback.failure.get().is_some() { return Err(MediaError::Closed); }
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

fn integer_property_matches(value:Object,expected:i64)->bool {
    if value.is_null() {return false;}
    // SAFETY: callers retain a live CF object through this inspection. Check
    // its type before number APIs and reject floating values rather than
    // accepting rounded/coerced readback as proof of an integer property.
    unsafe {
        let mut actual=0i64;
        CFGetTypeID(value)==CFNumberGetTypeID() && !CFNumberIsFloatType(value)
            && CFNumberGetValue(value,4,(&mut actual as *mut i64).cast()) && actual==expected
    }
}

fn burst_property_matches(value:Object,bytes:i64,seconds:f64)->bool {
    if value.is_null() {return false;}
    // SAFETY: callers hold a retained CF object. Inspect every container and
    // element before typed APIs; booleans and non-finite durations are rejected.
    unsafe {
        if CFGetTypeID(value)!=CFArrayGetTypeID() || CFArrayGetCount(value)!=2 {return false;}
        let size=CFArrayGetValueAtIndex(value,0);let duration=CFArrayGetValueAtIndex(value,1);let mut actual=0f64;
        let mut actual_bytes=0f64;
        !size.is_null() && CFGetTypeID(size)==CFNumberGetTypeID()
            && CFNumberGetValue(size,6,(&mut actual_bytes as *mut f64).cast()) && actual_bytes==bytes as f64
            && !duration.is_null() && CFGetTypeID(duration)==CFNumberGetTypeID()
            && CFNumberGetValue(duration,6,(&mut actual as *mut f64).cast()) && actual.is_finite() && actual==seconds
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
    if result.is_err() || idle.is_err() { state.fail(MediaError::Closed); }
}

fn output_inner(state: &CallbackState, status: i32, flags: u32, sample: Object) {
    state.pending.fetch_sub(1, Ordering::AcqRel);
    if status != 0 {state.fail(MediaError::Platform {operation:"encoder output callback",code:status});return;}
    if flags & 2 != 0 {state.fail(MediaError::Platform {operation:"encoder dropped output",code:flags as i32});return;}
    if sample.is_null() {state.fail(MediaError::InvalidFrame);return;}
    let parsed = unsafe { read_sample(sample,state.codec) };
    match parsed {
        Ok(frame) => state.publish(frame),
        Err(error) => state.fail(error),
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

#[cfg(test)]
mod tests;
