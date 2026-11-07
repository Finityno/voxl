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
    fn CVPixelBufferGetBaseAddressOfPlane(buffer:Object,plane:usize)->*const u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer:Object,plane:usize)->usize;
}

/// Relative to the previous successfully observed image, never a sent or
/// decoded image. Initial includes geometry, range or colour-tag changes.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum FrameComparison {Initial,Equal,Different}

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
