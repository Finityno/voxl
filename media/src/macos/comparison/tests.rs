use super::*;

#[link(name="CoreVideo",kind="framework")]
unsafe extern "C" {
    fn CVPixelBufferCreate(allocator:Object,width:usize,height:usize,format:u32,attributes:Object,out:*mut Object)->i32;
    fn CVBufferSetAttachment(buffer:Object,key:Object,value:Object,mode:u32);
    static kCVPixelBufferBytesPerRowAlignmentKey:Object;
    static kCVImageBufferTransferFunction_ITU_R_709_2:Object;
    static kCVImageBufferTransferFunction_sRGB:Object;
}

fn image(size:(usize,usize),change:Option<(usize,usize)>,padding:bool,full:bool,transfer:Option<Object>)->PixelBuffer {
    let (width,height)=size;let mut raw=ptr::null();
    // SAFETY: this fixture exclusively owns the Create reference while writing
    // its rows and colour attachment. Every buffer is immutable after from_raw.
    unsafe {
        let attributes=CFDictionaryCreateMutable(ptr::null(),0,ptr::null(),ptr::null());
        assert!(!attributes.is_null());let alignment=if padding {512i32} else {128};
        let number=CFNumberCreate(ptr::null(),9,(&alignment as *const i32).cast());assert!(!number.is_null());
        CFDictionarySetValue(attributes,kCVPixelBufferBytesPerRowAlignmentKey,number);
        let status=CVPixelBufferCreate(ptr::null(),width,height,u32::from_be_bytes(if full {*b"420f"} else {*b"420v"}),attributes,&mut raw);
        CFRelease(attributes);CFRelease(number);assert_eq!(status,0);assert!(!raw.is_null());
        assert_eq!(CVPixelBufferLockBaseAddress(raw,0),0);
        for (plane,rows) in [(0,height),(1,height/2)] {
            let base=CVPixelBufferGetBaseAddressOfPlane(raw,plane);let stride=CVPixelBufferGetBytesPerRowOfPlane(raw,plane);assert!(!base.is_null() && stride>=width);
            for row in 0..rows {
                ptr::write_bytes(base.add(row*stride),if padding {0xa5} else {0x5a},stride);
                for column in 0..width {*base.add(row*stride+column)=((row*width+column+plane*73)%256) as u8;}
            }
            if let Some((target,index))=change && target==plane {assert!(index<rows*width);*base.add(index/width*stride+index%width)^=1;}
        }
        assert_eq!(CVPixelBufferUnlockBaseAddress(raw,0),0);
        if let Some(transfer)=transfer {CVBufferSetAttachment(raw,kCVImageBufferTransferFunctionKey,transfer,1);}
        let image=PixelBuffer::from_raw(raw).unwrap();CFRelease(raw);image
    }
}

#[test]
fn native_frame_observer_exact_bits_updates_the_observed_baseline() {
    let mut observer=FrameObserver::new(64*1024).unwrap();let size=(66,34);
    let first=image(size,None,false,true,None);
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Equal));
    for change in [(0,0),(0,66*34-1),(1,0),(1,66*17-1)] {
        let candidate=image(size,Some(change),false,true,None);
        assert_eq!(observer.observe(&candidate),Ok(FrameComparison::Different));
        assert_eq!(observer.observe(&candidate),Ok(FrameComparison::Equal));
        assert_eq!(observer.observe(&first),Ok(FrameComparison::Different));
    }
    assert_eq!(observer.snapshot_bytes(),66*34*3/2);
    assert_eq!(observer.snapshot_capacity(),observer.snapshot_bytes());
    observer.reset();assert_eq!((observer.snapshot_bytes(),observer.snapshot_capacity()),(0,0));
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
}

#[test]
fn native_frame_observer_ignores_padding_and_different_plane_strides() {
    let first=image((66,34),None,false,true,None);let padded=image((66,34),None,true,true,None);
    // SAFETY: read-only scalar queries of retained immutable test images.
    unsafe {for plane in 0..2 {assert_ne!(CVPixelBufferGetBytesPerRowOfPlane(first.0.as_ptr(),plane),CVPixelBufferGetBytesPerRowOfPlane(padded.0.as_ptr(),plane));}}
    let mut observer=FrameObserver::new(64*1024).unwrap();
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
    for _ in 0..3 {assert_eq!(observer.observe(&padded),Ok(FrameComparison::Equal));assert_eq!(observer.observe(&first),Ok(FrameComparison::Equal));}
}

#[test]
fn native_frame_observer_geometry_range_and_colour_changes_start_new_snapshots() {
    let mut observer=FrameObserver::new(64*1024).unwrap();
    // SAFETY: the two linked colour strings are immortal framework constants.
    let transfers=unsafe {[kCVImageBufferTransferFunction_sRGB,kCVImageBufferTransferFunction_ITU_R_709_2]};
    for candidate in [image((66,34),None,false,true,None),image((32,16),None,false,true,None),image((32,16),None,false,false,None),
        image((32,16),None,false,false,Some(transfers[0])),image((32,16),None,false,false,Some(transfers[1])),image((32,16),None,false,false,None)] {
        assert_eq!(observer.observe(&candidate),Ok(FrameComparison::Initial));
        assert_eq!(observer.observe(&candidate),Ok(FrameComparison::Equal));
    }
}

#[test]
fn native_frame_observer_cap_and_bad_attachment_errors_clear_the_snapshot() {
    let first=image((32,16),None,false,true,None);let large=image((66,34),None,false,true,None);
    let mut observer=FrameObserver::new(1024).unwrap();assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
    assert_eq!(observer.observe(&large),Err(MediaError::Backpressure));assert_eq!(observer.snapshot_bytes(),0);
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
    // SAFETY: deliberately malformed metadata borrows an immortal CFBoolean;
    // the observer must type-check it before treating it as a colour string.
    let invalid=image((32,16),None,false,true,Some(unsafe {kCFBooleanTrue}));
    assert_eq!(observer.observe(&invalid),Err(MediaError::InvalidFrame));assert_eq!(observer.snapshot_bytes(),0);
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
    assert!(FrameObserver::new(0).is_err());assert!(FrameObserver::new(MAX_SNAPSHOT_BYTES+1).is_err());
}

#[test]
fn native_frame_observer_rejects_non_nv12_without_reusing_the_previous_snapshot() {
    let first=image((32,16),None,false,true,None);let mut observer=FrameObserver::new(4096).unwrap();
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
    let mut raw=ptr::null();
    // SAFETY: this Create reference is retained into the immutable wrapper and
    // released exactly once. The unsupported BGRA image is never read as NV12.
    let invalid=unsafe {
        assert_eq!(CVPixelBufferCreate(ptr::null(),32,16,u32::from_be_bytes(*b"BGRA"),ptr::null(),&mut raw),0);
        let image=PixelBuffer::from_raw(raw).unwrap();CFRelease(raw);image
    };
    assert_eq!(observer.observe(&invalid),Err(MediaError::InvalidFrame));assert_eq!(observer.snapshot_bytes(),0);
    assert_eq!(observer.observe(&first),Ok(FrameComparison::Initial));
}

#[test]
fn native_frame_fidelity_exact_plane_errors_ignore_padding_and_never_convert_color() {
    let source=image((66,34),None,false,true,None);let padded=image((66,34),None,true,true,None);
    let identical=compare_frames(&source,&padded).unwrap();assert!(!identical.color_tags_complete);
    assert_eq!(identical.planes.map(|plane|plane.samples),[2244,561,561]);
    assert!(identical.planes.iter().all(|plane|plane.changed==0 && plane.maximum==0 && plane.absolute==0 && plane.squared==0));
    assert_eq!(compare_frames(&source,&source),Ok(identical));
    for (change,target) in [((0,0),0),((0,66*34-1),0),((1,0),1),((1,66*17-1),2)] {
        let changed=image((66,34),Some(change),true,true,None);let result=compare_frames(&source,&changed).unwrap();
        for (index,error) in result.planes.iter().enumerate() {assert_eq!((error.changed,error.maximum,error.absolute,error.squared),if index==target {(1,1,1,1)} else {(0,0,0,0)});}
    }
    // SAFETY: these are linked immortal colour constants borrowed by the fixture.
    let transfer=unsafe {kCVImageBufferTransferFunction_sRGB};
    for other in [image((32,16),None,false,true,None),image((66,34),None,false,false,None),image((66,34),None,false,true,Some(transfer))] {
        assert_eq!(compare_frames(&source,&other),Err(MediaError::InvalidFrame));
    }
}

#[test]
fn native_nv12_visitor_has_canonical_active_order_and_preserves_missing_colour_metadata() {
    let source=image((66,34),None,false,true,None);let padded=image((66,34),None,true,true,None);let mut a=Vec::new();let mut b=Vec::new();
    let first=visit_nv12_bytes(&source,|row|a.extend_from_slice(row)).unwrap();let second=visit_nv12_bytes(&padded,|row|b.extend_from_slice(row)).unwrap();assert_eq!(a,b);assert_eq!(a.len(),66*34*3/2);assert_eq!(first,second);
    assert_eq!(first,Nv12ByteInfo {width:66,height:34,full_range:true,primaries:None,transfer:None,matrix:None});assert_eq!(a[66*34],73);
    // SAFETY: these linked SDK strings are immortal, read-only attachments.
    let tagged=image((66,34),None,false,false,Some(unsafe {kCVImageBufferTransferFunction_sRGB}));
    let info=visit_nv12_bytes(&tagged,|_|{}).unwrap();assert!(!info.full_range);assert_eq!(info.transfer,Some("iec61966-2-1"));assert_eq!(info.primaries,None);assert_eq!(info.matrix,None);
}

#[test]
fn native_nv12_visitor_releases_the_lock_on_panic_and_rejects_invalid_tags_before_visiting() {
    let source=image((66,34),None,false,true,None);
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||visit_nv12_bytes(&source,|_|panic!("intentional visitor failure"))));assert!(result.is_err());
    let mut rows=0;assert!(visit_nv12_bytes(&source,|_|rows+=1).is_ok());assert_eq!(rows,51);
    // SAFETY: a deliberately invalid tag borrows an immortal CFBoolean; the
    // visitor must reject it before exposing any row to the caller.
    let invalid=image((66,34),None,false,true,Some(unsafe {kCFBooleanTrue}));
    assert_eq!(visit_nv12_bytes(&invalid,|_|panic!("invalid metadata exposed pixels")),Err(MediaError::InvalidFrame));
}
