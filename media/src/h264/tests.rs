use super::*;

struct Writer(Vec<bool>);
impl Writer {
    fn bit(&mut self,v:bool) {self.0.push(v);}
    fn bits(&mut self,v:u32,n:usize) {for i in (0..n).rev() {self.bit(v&(1<<i)!=0);}}
    fn ue(&mut self,v:u32) {let code=u64::from(v)+1;let n=64-code.leading_zeros() as usize;for _ in 1..n {self.bit(false);}for i in (0..n).rev() {self.bit(code&(1<<i)!=0);}}
    fn se(&mut self,v:i32) {self.ue(if v<=0 {v.unsigned_abs()*2} else {v as u32*2-1});}
    fn finish(mut self)->Vec<u8> {
        self.bit(true);while !self.0.len().is_multiple_of(8) {self.bit(false);}
        let mut bytes=vec![0x67];let mut zeros=0;
        for chunk in self.0.as_chunks::<8>().0 {
            let value=chunk.iter().fold(0u8,|v,b|(v<<1)|u8::from(*b));if zeros==2 && value<=3 {bytes.push(3);zeros=0;}
            bytes.push(value);zeros=if value==0 {zeros+1} else {0};
        }bytes
    }
}
fn prefix(profile:u8,complex:bool)->Writer {
    let mut w=Writer(Vec::new());w.bits(u32::from(profile),8);w.bits(0,8);w.bits(31,8);w.ue(0);
    if profile==100 {w.ue(1);w.ue(0);w.ue(0);w.bit(false);w.bit(false);}
    if profile==244 {
        w.ue(3);w.bit(false);w.ue(2);w.ue(2);w.bit(false);w.bit(true);
        for index in 0..12 {w.bit(true);for _ in 0..if index<6 {16} else {64} {w.se(0);}}
    }
    w.ue(0);
    if complex {w.ue(1);w.bit(false);w.se(-1);w.se(1);w.ue(2);w.se(-2);w.se(2);} else {w.ue(0);w.ue(0);}
    w.ue(1);w.bit(false);w.ue(19);w.ue(14);w.bit(!complex);if complex {w.bit(true);}w.bit(true);
    w.bit(complex);if complex {for _ in 0..4 {w.ue(0);}}
    w
}
fn explicit(profile:u8,full:bool,description:Option<[u8;3]>,location:Option<[u8;2]>,complex:bool)->Vec<u8> {
    let mut w=prefix(profile,complex);w.bit(true);w.bit(complex);
    if complex {w.bits(255,8);w.bits(256,16);w.bits(1,16);}w.bit(complex);if complex {w.bit(true);}
    w.bit(true);w.bits(5,3);w.bit(full);w.bit(description.is_some());if let Some(values)=description {for v in values {w.bits(u32::from(v),8);}}
    w.bit(location.is_some());if let Some(values)=location {for v in values {w.ue(u32::from(v));}}
    w.finish()
}
#[test]
fn h264_sps_signal_reads_literal_colour_range_and_chroma_location() {
    for full in [false,true] {for values in [[1,1,1],[1,13,1],[2,2,2],[255,254,253]] {
        let value=sps_signal(&explicit(66,full,Some(values),Some([0,5]),false)).unwrap();
        assert_eq!(value,SpsSignal {profile_idc:66,level_idc:31,video_signal:Some(VideoSignal {video_format:5,full_range:full,colour_description:Some(values)}),chroma_location:Some([0,5])});
    }}
}
#[test]
fn h264_sps_signal_does_not_infer_missing_vui_or_colour_description() {
    let mut w=prefix(66,false);w.bit(false);let expected=SpsSignal {profile_idc:66,level_idc:31,video_signal:None,chroma_location:None};assert_eq!(sps_signal(&w.finish()).unwrap(),expected);
    let mut w=prefix(77,false);w.bit(true);for _ in 0..4 {w.bit(false);}assert_eq!(sps_signal(&w.finish()).unwrap(),SpsSignal {profile_idc:77,..expected});
    let value=sps_signal(&explicit(88,true,None,None,false)).unwrap();assert_eq!(value.video_signal,Some(VideoSignal {video_format:5,full_range:true,colour_description:None}));assert_eq!(value.chroma_location,None);
}
#[test]
fn h264_sps_signal_skips_high_profile_scaling_poc_cropping_and_extended_aspect() {
    for profile in [100,244] {let value=sps_signal(&explicit(profile,false,Some([9,16,9]),Some([3,4]),true)).unwrap();assert_eq!(value.profile_idc,profile);assert_eq!(value.video_signal.unwrap().colour_description,Some([9,16,9]));assert_eq!(value.chroma_location,Some([3,4]));}
}
#[test]
fn h264_sps_signal_rejects_truncated_prefix_invalid_escapes_and_oversized_input() {
    let valid=explicit(66,true,Some([1,13,1]),Some([5,5]),true);
    for size in 0..valid.len()-1 {assert!(sps_signal(&valid[..size]).is_err(),"truncated prefix length {size}");}
    for bad in [vec![0x67,0,0,0,1],vec![0x67,0,0,3],vec![0x67,0,0,3,4],vec![0x67;MAX_SPS_BYTES+1]] {assert!(sps_signal(&bad).is_err());}
    for header in [0x07,0x68,0xe7] {let mut bad=valid.clone();bad[0]=header;assert!(sps_signal(&bad).is_err());}
    let mut bad=valid;bad[2]|=1;assert!(sps_signal(&bad).is_err());
}
#[test]
fn h264_sps_signal_exp_golomb_and_chroma_locations_are_bounded() {
    let bytes=[0u8;8];assert!(Bits {bytes:&bytes,position:0}.ue(u32::MAX).is_err());
    assert!(Bits {bytes:&[0xff],position:7}.read(2).is_err());assert!(Bits {bytes:&bytes,position:0}.read(33).is_err());
    assert!(sps_signal(&explicit(66,false,None,Some([6,0]),false)).is_err());
    let mut unsupported=explicit(66,false,None,None,false);unsupported[1]=99;assert_eq!(sps_signal(&unsupported).unwrap_err(),MediaError::UnsupportedCodec);
}
#[test]
fn h264_sps_signal_removes_legal_emulation_prevention_without_claiming_full_validation() {
    // Colour codes are literal, including zero. The inserted escape must not
    // shift the later location codes. Later VUI syntax is intentionally opaque.
    let mut data=explicit(66,true,Some([0,0,1]),Some([1,2]),true);assert!(data.windows(3).any(|bytes|bytes==[0,0,3]));
    let before=sps_signal(&data).unwrap();data.extend_from_slice(&[0xff,0xff]);assert_eq!(sps_signal(&data).unwrap(),before);assert_eq!(before.video_signal.unwrap().colour_description,Some([0,0,1]));
}
