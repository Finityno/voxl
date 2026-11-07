//! Exact, opt-in observation of consecutive immutable NV12 images. This is
//! not a publication baseline, decoder-quality check or frame-skipping policy.
use super::{PixelBuffer,SourceColor,MediaError,ffi::*};
use std::{ptr,slice};

const MAX_SNAPSHOT_BYTES:usize=64*1024*1024;
const READ_ONLY:u64=1;

#[link(name="CoreVideo",kind="framework")]
unsafe extern "C" {
    fn CVPixelBufferGetPlaneCount(buffer:Object)->usize;
    fn CVPixelBufferGetWidthOfPlane(buffer:Object,plane:usize)->usize;
    fn CVPixelBufferGetHeightOfPlane(buffer:Object,plane:usize)->usize;
    fn CVPixelBufferLockBaseAddress(buffer:Object,flags:u64)->i32;
    fn CVPixelBufferUnlockBaseAddress(buffer:Object,flags:u64)->i32;
    fn CVPixelBufferGetBaseAddressOfPlane(buffer:Object,plane:usize)->*mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer:Object,plane:usize)->usize;
}

/// Relative to the previous successfully observed image, never a sent or
/// decoded image. Initial includes geometry, range or colour-tag changes.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum FrameComparison {Initial,Equal,Different}

/// Exact errors in active 8-bit samples, with no range/colour conversion.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct PlaneDifference {
    pub samples:u64,
    pub changed:u64,
    pub maximum:u8,
    pub absolute:u64,
    pub squared:u64,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct FrameDifference {
    pub width:u32,
    pub height:u32,
    /// Missing tags remain missing; matching absent tags are not colour proof.
    pub color_tags_complete:bool,
    /// Y, Cb, Cr separately; padding and alpha never enter these counters.
    pub planes:[PlaneDifference;3],
}

/// Metadata for active NV12 bytes. Unknown colour tags remain unknown rather
/// than being inferred from geometry. Strings use the unambiguous WebCodecs
/// names supported here; additional/missing native tags return None.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Nv12ByteInfo {
    pub width:u32,pub height:u32,pub full_range:bool,
    pub primaries:Option<&'static str>,pub transfer:Option<&'static str>,pub matrix:Option<&'static str>,
}
fn known_tag(actual:Object,known:&[(&std::ffi::CStr,&'static str)])->Option<&'static str> {
    if actual.is_null() {return None;}
    known.iter().find_map(|(symbol,name)| {
        let expected=super::optional_constant(symbol)?;
        // SAFETY: SourceColor validated and owns actual; the optional SDK data
        // symbol is a linked immortal CFString. Equality reads neither pixels.
        unsafe {CFEqual(actual,expected)}.then_some(*name)
    })
}

/// Visit active Y rows followed by interleaved CbCr rows in canonical packed
/// NV12 order. Padding is excluded and no full image is allocated/retained.
/// Row borrows cannot escape the callback; panic/error unwinding unlocks the
/// native image. The caller controls hashing or any other use of read-only data.
pub fn visit_nv12_bytes(image:&PixelBuffer,mut visit:impl FnMut(&[u8]))->Result<Nv12ByteInfo,MediaError> {
    let info=Info::read(image,MAX_SNAPSHOT_BYTES)?;let color=SourceColor::copy_from(image.0.as_ptr())?;
    let metadata=Nv12ByteInfo {width:info.width as u32,height:info.height as u32,full_range:info.format==u32::from_be_bytes(*b"420f"),
        primaries:known_tag(color.tags[0],&[(c"kCVImageBufferColorPrimaries_ITU_R_709_2","bt709"),(c"kCVImageBufferColorPrimaries_P3_D65","smpte432")]),
        transfer:known_tag(color.tags[1],&[(c"kCVImageBufferTransferFunction_sRGB","iec61966-2-1"),(c"kCVImageBufferTransferFunction_ITU_R_709_2","bt709")]),
        matrix:known_tag(color.tags[2],&[(c"kCVImageBufferYCbCrMatrix_ITU_R_709_2","bt709")])};
    with_planes(image,info,|planes| {
        for plane in planes {for row in 0..plane.rows {
            // SAFETY: the successful read lock and validated active row span
            // remain held for this synchronous callback only.
            visit(unsafe {slice::from_raw_parts(plane.base.add(row*plane.stride),info.width)});
        }}Ok(())
    })?;Ok(metadata)
}

/// Compare two immutable NV12 images in RAM. Geometry, pixel range and colour
/// tags must match exactly. No pixels are retained or exported. These numeric
/// errors alone do not establish perceptual quality or a production threshold.
pub fn compare_frames(source:&PixelBuffer,decoded:&PixelBuffer)->Result<FrameDifference,MediaError> {
    let info=Info::read(source,MAX_SNAPSHOT_BYTES)?;
    if Info::read(decoded,MAX_SNAPSHOT_BYTES)?!=info {return Err(MediaError::InvalidFrame);}
    let color=SourceColor::copy_from(source.0.as_ptr())?;
    if !color.matches(decoded.0.as_ptr()) {return Err(MediaError::InvalidFrame);}
    let mut result=FrameDifference {width:info.width as u32,height:info.height as u32,color_tags_complete:color.tags.iter().all(|tag|!tag.is_null()),planes:[PlaneDifference::default();3]};
    result.planes[0].samples=(info.width*info.height) as u64;
    result.planes[1].samples=(info.width*info.height/4) as u64;result.planes[2].samples=result.planes[1].samples;
    if source.0==decoded.0 {return Ok(result);}
    with_planes(source,info,|source|with_planes(decoded,info,|decoded| {
        for (plane,(source,decoded)) in source.iter().zip(&decoded).enumerate() {
            for row in 0..source.rows {
                // SAFETY: both read locks cover their independently validated
                // row strides/spans. Only active bytes are read, never padding.
                let a=unsafe {slice::from_raw_parts(source.base.add(row*source.stride),info.width)};
                // SAFETY: the second image has matching geometry and a held lock.
                let b=unsafe {slice::from_raw_parts(decoded.base.add(row*decoded.stride),info.width)};
                for (column,(a,b)) in a.iter().zip(b).enumerate() {
                    let error=&mut result.planes[if plane==0 {0} else {1+column%2}];
                    let difference=a.abs_diff(*b);error.maximum=error.maximum.max(difference);
                    error.changed+=u64::from(difference!=0);error.absolute+=u64::from(difference);error.squared+=u64::from(difference)*u64::from(difference);
                }
            }
        }
        Ok(())
    }))?;
    Ok(result)
}

/// A synchronous observer with one bounded active-byte snapshot. Construction
/// does not allocate pixels. It retains copied colour strings, not source
/// surfaces. Call reset at each source/authority/recovery boundary. Every
/// observation error clears the snapshot. No result authorizes suppression.
pub struct FrameObserver {limit:usize,state:State}
enum State {Empty,Snapshot(Snapshot)}
struct Snapshot {info:Info,color:SourceColor,bytes:Vec<u8>}
#[derive(Clone,Copy,PartialEq,Eq)]
struct Info {width:usize,height:usize,format:u32,bytes:usize}
impl Info {
    fn read(image:&PixelBuffer,limit:usize)->Result<Self,MediaError> {
        let buffer=image.0.as_ptr();
        // SAFETY: PixelBuffer retains a valid immutable native image.
        let (width,height,format)=unsafe {(CVPixelBufferGetWidth(buffer),CVPixelBufferGetHeight(buffer),CVPixelBufferGetPixelFormatType(buffer))};
        if !(2..=8192).contains(&width) || !(2..=8192).contains(&height) || !width.is_multiple_of(2) || !height.is_multiple_of(2)
            || ![u32::from_be_bytes(*b"420f"),u32::from_be_bytes(*b"420v")].contains(&format) {return Err(MediaError::InvalidFrame);}
        let bytes=width.checked_mul(height).and_then(|value|value.checked_mul(3)).map(|value|value/2).ok_or(MediaError::InvalidFrame)?;
        if bytes>limit {return Err(MediaError::Backpressure);}
        // SAFETY: these scalar queries borrow this retained image only.
        unsafe {
            if CVPixelBufferGetPlaneCount(buffer)!=2 || CVPixelBufferGetWidthOfPlane(buffer,0)!=width || CVPixelBufferGetHeightOfPlane(buffer,0)!=height
                || CVPixelBufferGetWidthOfPlane(buffer,1)!=width/2 || CVPixelBufferGetHeightOfPlane(buffer,1)!=height/2 {return Err(MediaError::InvalidFrame);}
        }
        Ok(Self {width,height,format,bytes})
    }
}

struct ReadLock<'a>(&'a PixelBuffer);
impl ReadLock<'_> {
    fn finish(self)->Result<(),MediaError> {
        // SAFETY: this guard owns one successful read-only lock. Forgetting it
        // after unlock prevents a second unlock; the borrowed image stays live.
        let code=unsafe {CVPixelBufferUnlockBaseAddress(self.0.0.as_ptr(),READ_ONLY)};
        std::mem::forget(self);
        if code==0 {Ok(())} else {Err(MediaError::Platform {operation:"comparison unlock",code})}
    }
}
impl Drop for ReadLock<'_> {
    fn drop(&mut self) {
        // SAFETY: an early return or panic still owns the successful lock.
        unsafe {CVPixelBufferUnlockBaseAddress(self.0.0.as_ptr(),READ_ONLY);}
    }
}
struct Plane {base:*const u8,stride:usize,rows:usize}
fn with_planes<T>(image:&PixelBuffer,info:Info,body:impl FnOnce([Plane;2])->Result<T,MediaError>)->Result<T,MediaError> {
    // SAFETY: image owns an immutable native surface. CPU access is enclosed
    // by the matching read-only lock and never escapes this call.
    let code=unsafe {CVPixelBufferLockBaseAddress(image.0.as_ptr(),READ_ONLY)};
    if code!=0 {return Err(MediaError::Platform {operation:"comparison lock",code});}
    let locked=ReadLock(image);
    let plane=|index:usize|->Result<Plane,MediaError> {
        // SAFETY: Info checked the plane geometry; the read lock is held.
        let (base,stride)=unsafe {(CVPixelBufferGetBaseAddressOfPlane(image.0.as_ptr(),index),CVPixelBufferGetBytesPerRowOfPlane(image.0.as_ptr(),index))};
        let rows=if index==0 {info.height} else {info.height/2};
        let span=(rows-1).checked_mul(stride).and_then(|value|value.checked_add(info.width));
        if base.is_null() || stride<info.width || span.is_none_or(|value|value>isize::MAX as usize) {return Err(MediaError::InvalidFrame);}
        Ok(Plane {base,stride,rows})
    };
    let result=(||body([plane(0)?,plane(1)?]))();
    let unlocked=locked.finish();
    unlocked?;result
}

impl FrameObserver {
    pub fn new(max_snapshot_bytes:usize)->Result<Self,MediaError> {
        if !(1..=MAX_SNAPSHOT_BYTES).contains(&max_snapshot_bytes) {return Err(MediaError::Configuration);}
        Ok(Self {limit:max_snapshot_bytes,state:State::Empty})
    }
    pub fn reset(&mut self) {self.state=State::Empty;}
    /// Active bytes, excluding native source surfaces and copied colour tags.
    pub fn snapshot_bytes(&self)->usize {match &self.state {State::Empty=>0,State::Snapshot(snapshot)=>snapshot.bytes.len()}}
    /// Vec capacity, not allocator/driver or total process memory.
    pub fn snapshot_capacity(&self)->usize {match &self.state {State::Empty=>0,State::Snapshot(snapshot)=>snapshot.bytes.capacity()}}
    pub fn observe(&mut self,image:&PixelBuffer)->Result<FrameComparison,MediaError> {
        // Take ownership first: errors and unwinding leave one empty state.
        let previous=std::mem::replace(&mut self.state,State::Empty);
        let info=Info::read(image,self.limit)?;
        let (mut snapshot,initial)=match previous {
            State::Snapshot(snapshot) if snapshot.info==info && snapshot.color.matches(image.0.as_ptr())=>(snapshot,false),
            old=>{
                // Release the old pixel allocation before allocating its
                // replacement, including at geometry/colour boundaries.
                drop(old);
                let color=SourceColor::copy_from(image.0.as_ptr())?;
                let mut bytes=Vec::new();bytes.try_reserve_exact(info.bytes).map_err(|_|MediaError::Backpressure)?;bytes.resize(info.bytes,0);
                (Snapshot {info,color,bytes},true)
            },
        };
        let different=with_planes(image,info,|planes| {
            let mut different=initial;let mut offset=0;
            if !different {
                'search: for plane in &planes {
                    for row in 0..plane.rows {
                        // SAFETY: the native lock and validated plane span
                        // cover this active row. No borrowed bytes are retained.
                        let input=unsafe {slice::from_raw_parts(plane.base.add(row*plane.stride),info.width)};
                        if snapshot.bytes[offset+row*info.width..offset+(row+1)*info.width]!=*input {different=true;break 'search;}
                    }
                    offset+=plane.rows*info.width;
                }
            }
            if different {
                offset=0;
                for plane in &planes {
                    for row in 0..plane.rows {
                        // SAFETY: the owned allocation is exactly the active
                        // Y+UV byte span and cannot alias the native source.
                        unsafe {ptr::copy_nonoverlapping(plane.base.add(row*plane.stride),snapshot.bytes.as_mut_ptr().add(offset+row*info.width),info.width);}
                    }
                    offset+=plane.rows*info.width;
                }
            }
            Ok(different)
        })?;
        self.state=State::Snapshot(snapshot);
        Ok(if initial {FrameComparison::Initial} else if different {FrameComparison::Different} else {FrameComparison::Equal})
    }
}

#[cfg(test)]
mod tests;
