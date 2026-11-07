//! Installed CoreMedia/VideoToolbox C ABI, used only by the bounded qualifier.
use super::{Object,Session,ArrayCallbacks,CMTime,c_void};
#[repr(C)]
pub struct Timing {pub duration:CMTime,pub presentation:CMTime,pub decode:CMTime}
#[repr(C)]
pub struct Dimensions {pub width:i32,pub height:i32}
#[repr(C)]
pub struct DecodeCallback {
    pub callback:Option<unsafe extern "C" fn(*mut c_void,*mut c_void,i32,u32,Object,CMTime,CMTime)>,
    pub context:*mut c_void,
}
#[repr(C)]
pub struct KeyCallbacks {
    pub values:ArrayCallbacks,
    pub hash:Option<unsafe extern "C" fn(Object)->usize>,
}
#[link(name="CoreFoundation",kind="framework")]
unsafe extern "C" {
    pub static kCFTypeDictionaryKeyCallBacks:KeyCallbacks;
    pub static kCFTypeDictionaryValueCallBacks:ArrayCallbacks;
}
#[link(name="CoreVideo",kind="framework")]
unsafe extern "C" {pub static kCVPixelBufferPixelFormatTypeKey:Object;}
#[link(name="CoreMedia",kind="framework")]
unsafe extern "C" {
    pub fn CMVideoFormatDescriptionCreateFromH264ParameterSets(allocator:Object,count:usize,pointers:*const *const u8,sizes:*const usize,nal_length:i32,out:*mut Object)->i32;
    pub fn CMVideoFormatDescriptionGetDimensions(format:Object)->Dimensions;
    pub fn CMBlockBufferCreateWithMemoryBlock(allocator:Object,block:*mut c_void,length:usize,block_allocator:Object,custom:Object,offset:usize,data_length:usize,flags:u32,out:*mut Object)->i32;
    pub fn CMBlockBufferReplaceDataBytes(source:Object,block:Object,offset:usize,length:usize)->i32;
    pub fn CMSampleBufferCreateReady(allocator:Object,block:Object,format:Object,count:isize,timing_count:isize,timing:*const Timing,size_count:isize,sizes:*const usize,out:*mut Object)->i32;
}
#[link(name="VideoToolbox",kind="framework")]
unsafe extern "C" {
    pub fn VTDecompressionSessionCreate(allocator:Object,format:Object,specification:Object,attributes:Object,callback:*const DecodeCallback,out:*mut Session)->i32;
    pub fn VTDecompressionSessionDecodeFrame(session:Session,sample:Object,flags:u32,source:*mut c_void,info:*mut u32)->i32;
    pub fn VTDecompressionSessionInvalidate(session:Session);
}
