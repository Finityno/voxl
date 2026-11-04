//! The small system C ABI used by the encoder. Signatures follow the installed
//! Apple SDK; CMTime comes from the generated framework bindings.
#![allow(non_snake_case, non_upper_case_globals)]
use std::ffi::c_void;
use objc2_core_media::CMTime;
pub type Object = *const c_void;
pub type Session = *mut c_void;
pub type Output = unsafe extern "C" fn(*mut c_void, *mut c_void, i32, u32, Object);

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
    pub static kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder: Object;
    pub static kVTEncodeFrameOptionKey_ForceKeyFrame: Object;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRetain(value: Object) -> Object;
    pub fn CFRelease(value: Object);
    pub fn CFDictionaryCreateMutable(allocator: Object, capacity: isize, key_callbacks: Object, value_callbacks: Object) -> Session;
    pub fn CFDictionarySetValue(dictionary: Session, key: Object, value: Object);
    pub fn CFNumberCreate(allocator: Object, kind: isize, value: *const c_void) -> Object;
    pub fn CFBooleanGetValue(value: Object) -> bool;
    pub static kCFBooleanTrue: Object;
    pub static kCFBooleanFalse: Object;
}
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    pub fn CVPixelBufferGetWidth(buffer: Object) -> usize;
    pub fn CVPixelBufferGetHeight(buffer: Object) -> usize;
    pub fn CVPixelBufferGetPixelFormatType(buffer: Object) -> u32;
}
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    pub fn CMSampleBufferGetDataBuffer(sample: Object) -> Object;
    pub fn CMSampleBufferGetFormatDescription(sample: Object) -> Object;
    pub fn CMSampleBufferGetPresentationTimeStamp(sample: Object) -> CMTime;
    pub fn CMBlockBufferGetDataLength(buffer: Object) -> usize;
    pub fn CMBlockBufferCopyDataBytes(buffer: Object, offset: usize, length: usize, target: *mut c_void) -> i32;
    pub fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format: Object, index: usize, bytes: *mut *const u8,
        length: *mut usize, count: *mut usize, nal_length: *mut i32) -> i32;
}
