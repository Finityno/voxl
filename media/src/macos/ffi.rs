//! The small system C ABI used by the encoder. Signatures follow the installed
//! Apple SDK; CMTime comes from the generated framework bindings.
#![allow(non_snake_case, non_upper_case_globals)]
use std::ffi::{c_char,c_void};
use objc2_core_media::CMTime;
pub type Object = *const c_void;
pub type Session = *mut c_void;
pub type Output = unsafe extern "C" fn(*mut c_void, *mut c_void, i32, u32, Object);
// VideoToolbox/VTErrors.h: an unrecognized or unsupported session property.
pub const kVTPropertyNotSupportedErr: i32 = -12900;
// Darwin dlfcn.h. Resolve optional newer data symbols without raising the
// embedding application's minimum supported OS through a strong import.
pub const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
#[link(name = "System")]
unsafe extern "C" {
    pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    pub fn VTCompressionSessionCreate(allocator: Object, width: i32, height: i32, codec: u32,
        specification: Object, attributes: Object, compressed_allocator: Object, callback: Option<Output>,
        context: *mut c_void, session: *mut Session) -> i32;
    pub fn VTCompressionSessionInvalidate(session: Session);
    pub fn VTCompressionSessionEncodeFrame(session: Session, image: Object, pts: CMTime, duration: CMTime,
        properties: Object, source: *mut c_void, flags: *mut u32) -> i32;
    pub fn VTSessionSetProperty(session: Session, key: Object, value: Object) -> i32;
    pub fn VTSessionCopyProperty(session: Session, key: Object, allocator: Object, out: *mut Object) -> i32;
    pub fn VTCompressionSessionPrepareToEncodeFrames(session: Session) -> i32;
    pub static kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: Object;
    pub static kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder: Object;
    pub static kVTCompressionPropertyKey_RealTime: Object;
    pub static kVTCompressionPropertyKey_AllowFrameReordering: Object;
    pub static kVTCompressionPropertyKey_AverageBitRate: Object;
    pub static kVTCompressionPropertyKey_ExpectedFrameRate: Object;
    pub static kVTCompressionPropertyKey_MaxKeyFrameInterval: Object;
    pub static kVTCompressionPropertyKey_ProfileLevel: Object;
    pub static kVTProfileLevel_H264_Baseline_AutoLevel: Object;
    pub static kVTProfileLevel_HEVC_Main_AutoLevel: Object;
    pub static kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder: Object;
    pub static kVTEncodeFrameOptionKey_ForceKeyFrame: Object;
    pub static kVTCompressionPropertyKey_ColorPrimaries: Object;
    pub static kVTCompressionPropertyKey_TransferFunction: Object;
    pub static kVTCompressionPropertyKey_YCbCrMatrix: Object;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRetain(value: Object) -> Object;
    pub fn CFRelease(value: Object);
    pub fn CFDictionaryCreateMutable(allocator: Object, capacity: isize, key_callbacks: Object, value_callbacks: Object) -> Session;
    pub fn CFDictionarySetValue(dictionary: Session, key: Object, value: Object);
    pub fn CFNumberCreate(allocator: Object, kind: isize, value: *const c_void) -> Object;
    pub fn CFBooleanGetValue(value: Object) -> bool;
    pub fn CFBooleanGetTypeID() -> usize;
    pub fn CFGetTypeID(value: Object) -> usize;
    pub fn CFStringGetTypeID() -> usize;
    pub fn CFStringCreateCopy(allocator: Object, value: Object) -> Object;
    pub fn CFEqual(left: Object, right: Object) -> bool;
    pub static kCFBooleanTrue: Object;
    pub static kCFBooleanFalse: Object;
}
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    pub fn CVPixelBufferGetWidth(buffer: Object) -> usize;
    pub fn CVPixelBufferGetHeight(buffer: Object) -> usize;
    pub fn CVPixelBufferGetPixelFormatType(buffer: Object) -> u32;
    // Borrowed get API keeps standalone support before macOS 12. The caller
    // retains an immutable buffer throughout all attachment reads.
    pub fn CVBufferGetAttachment(buffer: Object, key: Object, mode: *mut u32) -> Object;
    pub static kCVImageBufferColorPrimariesKey: Object;
    pub static kCVImageBufferTransferFunctionKey: Object;
    pub static kCVImageBufferYCbCrMatrixKey: Object;
}
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    pub fn CMSampleBufferGetDataBuffer(sample: Object) -> Object;
    pub fn CMSampleBufferGetFormatDescription(sample: Object) -> Object;
    pub fn CMSampleBufferGetPresentationTimeStamp(sample: Object) -> CMTime;
    pub fn CMBlockBufferGetDataLength(buffer: Object) -> usize;
    pub fn CMBlockBufferCopyDataBytes(buffer: Object, offset: usize, length: usize, target: *mut c_void) -> i32;
    pub fn CMVideoFormatDescriptionGetHEVCParameterSetAtIndex(format: Object, index: usize, bytes: *mut *const u8,
        length: *mut usize, count: *mut usize, nal_length: *mut i32) -> i32;
    pub fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format: Object, index: usize, bytes: *mut *const u8,
        length: *mut usize, count: *mut usize, nal_length: *mut i32) -> i32;
}
