//! Local H.264 source-fidelity qualification. Not a network decoder or provider
//! capability: synchronous, at most 512 frames, no pixel persistence/conversion.
use super::{PixelBuffer,ffi::*,check};
use crate::{EncodedFrame,MediaError,MAX_FRAME_BYTES,codec::VideoCodec};
use objc2_core_media::{CMTime,CMTimeFlags};
use std::{ffi::c_void,ptr,sync::Mutex};

mod ffi;
use ffi::*;
const MAX_PARAMETERS:usize=16*1024;
const MAX_NALS:usize=256;

struct Owned(Object);
impl Drop for Owned {fn drop(&mut self) {if !self.0.is_null() {
    // SAFETY: every Owned wraps exactly one nonnull Create reference.
    unsafe {CFRelease(self.0);}
}}}
#[derive(PartialEq,Eq)]
struct Parameters {sps:Vec<u8>,pps:Vec<u8>}
struct AccessUnit<'a> {nals:Vec<&'a [u8]>,parameters:Option<Parameters>,idr:bool}
impl<'a> AccessUnit<'a> {
    fn read(data:&'a [u8])->Result<Self,MediaError> {
        if data.is_empty() || data.len()>MAX_FRAME_BYTES || !data.starts_with(&[0,0,0,1]) {return Err(MediaError::InvalidFrame);}
        // The fork encoder emits four-byte start codes, never a mixed external
        // Annex-B stream. Reject unsupported framing instead of guessing.
        let mut nals=Vec::new();let mut start=4;let mut sps=None;let mut pps=None;let mut idr=false;let mut picture=false;
        loop {
            let end=data[start..].windows(4).position(|v|v==[0,0,0,1]).map_or(data.len(),|offset|start+offset);
            let nal=&data[start..end];
            if nals.len()>=MAX_NALS || nal.is_empty() || nal[0]&0x80!=0 {return Err(MediaError::InvalidFrame);}
            match nal[0]&31 {
                1=>picture=true,5=>{picture=true;idr=true;},
                7|8=>{
                    if nal.len()>MAX_PARAMETERS {return Err(MediaError::Backpressure);}
                    let target=if nal[0]&31==7 {&mut sps} else {&mut pps};
                    if target.is_some() {return Err(MediaError::InvalidFrame);}*target=Some(nal.to_vec());
                },
                6|9|12=>{},_=>return Err(MediaError::InvalidFrame),
            }
            nals.push(nal);if end==data.len() {break;}start=end+4;
        }
        if !picture || sps.is_some()!=pps.is_some() {return Err(MediaError::InvalidFrame);}
        Ok(Self {nals,parameters:sps.zip(pps).map(|(sps,pps)|Parameters {sps,pps}),idr})
    }
    fn length_prefixed(&self)->Vec<u8> {
        let mut data=Vec::with_capacity(self.nals.iter().map(|v|v.len()+4).sum());
        for nal in &self.nals {data.extend_from_slice(&(nal.len() as u32).to_be_bytes());data.extend_from_slice(nal);}data
    }
}

enum Output {Waiting,Ready(PixelBuffer,CMTime),Failed(MediaError)}
struct Slot {output:Mutex<Output>,idle:fn()}
unsafe extern "C" fn decoded(_context:*mut c_void,source:*mut c_void,status:i32,flags:u32,image:Object,pts:CMTime,_duration:CMTime) {
    if source.is_null() {return;}
    // SAFETY: flags zero requires callbacks to finish before DecodeFrame
    // returns. Its stack Slot remains live and is synchronized for callbacks.
    let slot=unsafe {&*source.cast::<Slot>()};
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut output=slot.output.lock().unwrap_or_else(|value|value.into_inner());
        let next=if !matches!(*output,Output::Waiting) {Output::Failed(MediaError::InvalidFrame)}
        else if status!=0 {Output::Failed(MediaError::Platform {operation:"quality decoder callback",code:status})}
        else if flags&2!=0 || image.is_null() {Output::Failed(MediaError::InvalidFrame)}
        else {
            // SAFETY: the framework's callback image is a live, immutable
            // decoded CVPixelBuffer. Retaining it extends its read-only life.
            match unsafe {PixelBuffer::from_raw(image)} {Ok(image)=>Output::Ready(image,pts),Err(error)=>Output::Failed(error)}
        };
        *output=next;
    }));
    // Match the embedding encoder's synchronous allocator idle contract. No
    // panic may escape either callback or the embedding hook across C.
    let idle=std::panic::catch_unwind(slot.idle);
    if result.is_err() || idle.is_err() {*slot.output.lock().unwrap_or_else(|value|value.into_inner())=Output::Failed(MediaError::Closed);}
}

struct Native {session:Session,format:Owned,parameters:Parameters,signal:crate::h264::SpsSignal}
impl Native {
    fn new(parameters:Parameters,size:(u32,u32),format:u32)->Result<Self,MediaError> {
        let signal=crate::h264::sps_signal(&parameters.sps)?;
        let pointers=[parameters.sps.as_ptr(),parameters.pps.as_ptr()];let sizes=[parameters.sps.len(),parameters.pps.len()];let mut description=ptr::null();
        // SAFETY: two bounded immutable parameter sets live through this Create
        // call. CoreMedia copies the parameter data into its owned description.
        let code=unsafe {CMVideoFormatDescriptionCreateFromH264ParameterSets(ptr::null(),2,pointers.as_ptr(),sizes.as_ptr(),4,&mut description)};
        let description=Owned(description);check("quality H264 format",code)?;
        if description.0.is_null() {return Err(MediaError::InvalidFrame);}
        // SAFETY: a successfully created video description has scalar dimensions.
        let dimensions=unsafe {CMVideoFormatDescriptionGetDimensions(description.0)};
        if (i64::from(dimensions.width),i64::from(dimensions.height))!=(i64::from(size.0),i64::from(size.1)) {return Err(MediaError::InvalidFrame);}
        // SAFETY: the framework callback structs own CF values in this dictionary;
        // the number/dictionary remain live through session creation.
        let attributes=Owned(unsafe {CFDictionaryCreateMutable(ptr::null(),1,ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast())}.cast());
        let number=Owned(unsafe {CFNumberCreate(ptr::null(),3,(&format as *const u32).cast())});
        if attributes.0.is_null() || number.0.is_null() {return Err(MediaError::Configuration);}
        // SAFETY: attributes is the mutable dictionary owned above. Type 3 is
        // CFNumber's signed 32-bit native pixel-format value (all below i32::MAX).
        unsafe {CFDictionarySetValue(attributes.0.cast_mut(),kCVPixelBufferPixelFormatTypeKey,number.0);}
        let callback=DecodeCallback {callback:Some(decoded),context:ptr::null_mut()};let mut session=ptr::null_mut();
        // SAFETY: synchronous decoding will supply each call's live Slot as
        // sourceFrameRefCon. There is no async callback context to retire.
        let code=unsafe {VTDecompressionSessionCreate(ptr::null(),description.0,ptr::null(),attributes.0,&callback,&mut session)};
        if code!=0 || session.is_null() {
            if !session.is_null() {
                // SAFETY: defensively retire any unexpected partial Create owner.
                unsafe {VTDecompressionSessionInvalidate(session);CFRelease(session.cast());}
            }
            check("quality decoder create",code)?;return Err(MediaError::Configuration);
        }
        Ok(Self {session,format:description,parameters,signal})
    }
    fn decode(&self,unit:&AccessUnit<'_>,pts:i64,idle:fn())->Result<PixelBuffer,MediaError> {
        let bytes=unit.length_prefixed();let mut block=ptr::null();
        // SAFETY: CoreMedia allocates its own bounded encoded-byte block. No
        // Rust allocation is lent to a possibly retaining native sample.
        let code=unsafe {CMBlockBufferCreateWithMemoryBlock(ptr::null(),ptr::null_mut(),bytes.len(),ptr::null(),ptr::null(),0,bytes.len(),0,&mut block)};
        let block=Owned(block);check("quality block create",code)?;if block.0.is_null() {return Err(MediaError::Configuration);}
        // SAFETY: the whole owned block is exactly bytes.len() bytes long.
        check("quality block copy",unsafe {CMBlockBufferReplaceDataBytes(bytes.as_ptr().cast(),block.0,0,bytes.len())})?;
        let invalid=CMTime {value:0,timescale:0,flags:CMTimeFlags::empty(),epoch:0};
        let timing=Timing {duration:invalid,presentation:CMTime {value:pts,timescale:1_000_000,flags:CMTimeFlags::Valid,epoch:0},decode:invalid};let length=bytes.len();let mut sample=ptr::null();
        // SAFETY: one sample, one timing and one size entry, all live for Create.
        let code=unsafe {CMSampleBufferCreateReady(ptr::null(),block.0,self.format.0,1,1,&timing,1,&length,&mut sample)};
        let sample=Owned(sample);check("quality sample create",code)?;if sample.0.is_null() {return Err(MediaError::Configuration);}
        let slot=Slot {output:Mutex::new(Output::Waiting),idle};let mut flags=0;
        // SAFETY: neither asynchronous nor temporal-processing flag is set.
        // Apple's header guarantees the callback completes before this returns.
        check("quality decode",unsafe {VTDecompressionSessionDecodeFrame(self.session,sample.0,0,(&slot as *const Slot).cast_mut().cast(),&mut flags)})?;
        if flags&2!=0 {return Err(MediaError::InvalidFrame);}
        match slot.output.into_inner().unwrap_or_else(|value|value.into_inner()) {
            Output::Ready(image,time) if time.epoch==0 && time.flags==CMTimeFlags::Valid && time.timescale>0 && i128::from(time.value)*1_000_000==i128::from(pts)*i128::from(time.timescale)=>Ok(image),
            Output::Failed(error)=>Err(error),_=>Err(MediaError::InvalidFrame),
        }
    }
}
impl Drop for Native {fn drop(&mut self) {
    // SAFETY: flags-zero decoding left no asynchronous calls. Invalidate before
    // releasing the Create reference; the format remains owned until afterward.
    unsafe {VTDecompressionSessionInvalidate(self.session);CFRelease(self.session.cast());}
}}
enum Phase {Initial,Active {native:Native,last_pts:i64,frames:u16},Closed}
/// Bounded serial local decode for source-quality experiments. It accepts only
/// the fork's four-byte-start-code H.264 output, beginning at a self-contained
/// IDR, with fixed parameter sets and geometry. Errors retire the entire chain.
/// It creates no Rust threads and never enables asynchronous/temporal decoding.
pub struct H264QualityDecoder {size:(u32,u32),format:u32,idle:fn(),phase:Phase}
impl H264QualityDecoder {
    pub fn new(width:u32,height:u32,full_range:bool,idle:fn())->Result<Self,MediaError> {
        if !(320..=4096).contains(&width) || !(180..=2160).contains(&height) || !width.is_multiple_of(2) || !height.is_multiple_of(2) || u64::from(width)*u64::from(height)>3840*2160 {return Err(MediaError::Configuration);}
        Ok(Self {size:(width,height),format:u32::from_be_bytes(if full_range {*b"420f"} else {*b"420v"}),idle,phase:Phase::Initial})
    }
    /// Literal SPS prefix metadata from the active bounded parameter sets.
    /// This does not change decode configuration or fill missing colour defaults.
    pub fn parameter_signal(&self)->Option<crate::h264::SpsSignal> {
        match &self.phase {Phase::Active {native,..}=>Some(native.signal),_=>None}
    }
    pub fn decode(&mut self,frame:&EncodedFrame)->Result<PixelBuffer,MediaError> {
        let phase=std::mem::replace(&mut self.phase,Phase::Closed);
        if matches!(phase,Phase::Closed) {return Err(MediaError::Closed);}
        if frame.codec!=VideoCodec::H264 || frame.presentation_time_micros<0 {return Err(MediaError::InvalidFrame);}
        let unit=AccessUnit::read(&frame.annex_b)?;if unit.idr!=frame.keyframe {return Err(MediaError::InvalidFrame);}
        let (native,frames)=match phase {
            Phase::Initial=>{
                if !unit.idr {return Err(MediaError::InvalidFrame);}
                let parameters=unit.parameters.as_ref().ok_or(MediaError::InvalidFrame)?;
                (Native::new(Parameters {sps:parameters.sps.clone(),pps:parameters.pps.clone()},self.size,self.format)?,0)
            },
            Phase::Active {native,last_pts,frames}=>{
                if frames>=512 || frame.presentation_time_micros<=last_pts || unit.parameters.as_ref().is_some_and(|parameters|*parameters!=native.parameters) {return Err(MediaError::InvalidFrame);}(native,frames)
            },
            Phase::Closed=>return Err(MediaError::Closed),
        };
        let image=native.decode(&unit,frame.presentation_time_micros,self.idle)?;
        // SAFETY: decoded retains an immutable CVPixelBuffer from the callback.
        if unsafe {(CVPixelBufferGetWidth(image.0.as_ptr()),CVPixelBufferGetHeight(image.0.as_ptr()),CVPixelBufferGetPixelFormatType(image.0.as_ptr()))}!=(self.size.0 as usize,self.size.1 as usize,self.format) {return Err(MediaError::InvalidFrame);}
        self.phase=Phase::Active {native,last_pts:frame.presentation_time_micros,frames:frames+1};Ok(image)
    }
    pub fn close(&mut self) {self.phase=Phase::Closed;}
}

#[cfg(test)]
mod tests;
