//! Single-layer, low-delay AV1 temporal units, kept separate from Annex B NALs.
//! The framing/profile checks run before the RTP packetizer, whose OBU parser
//! assumes complete size fields. They do not establish encoder/decoder quality.

use crate::{MediaError,MAX_FRAME_BYTES};

#[derive(Debug)]
pub struct Av1Frame {
    /// One low-overhead temporal unit, with a size field on every OBU. No IVF,
    /// Annex B temporal/frame-unit wrappers, RTP aggregation byte or SVC layers.
    pub temporal_unit:Vec<u8>,
    /// Supplied by the native encoder. Every random-access unit must include
    /// its sequence header so a new decoder/ICE generation can begin there.
    pub keyframe:bool,
    pub presentation_time_micros:i64,
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct Av1MainProfile {pub level_idx:u8,pub high_tier:bool}
impl Av1MainProfile {
    pub fn validate(self)->Result<(),MediaError> {
        // Defined, constrained levels from AV1 Annex A. Maximum-parameters
        // level 31 cannot describe a bounded desktop encoder capability.
        if !matches!(self.level_idx,0|1|4|5|8|9|12..=19) || self.high_tier && self.level_idx<8 {
            return Err(MediaError::UnsupportedCodec);
        }
        Ok(())
    }
    #[cfg(feature="webrtc")]
    pub(crate) fn fmtp(self)->String {format!("profile=0;level-idx={};tier={}",self.level_idx,u8::from(self.high_tier))}
    #[cfg(feature="webrtc")]
    pub(crate) fn receiver_supports(self,fmtp:&str)->bool {
        // RTP AV1 parameters are asymmetric receiver limits. A codec-name
        // match alone must never admit a stream above the answerer's limit.
        if fmtp.len()>4096 {return false;}
        let (mut profile,mut level,mut tier)=(None,None,None);
        for field in fmtp.split(';').filter(|field|!field.trim().is_empty()) {
            let field=field.trim();let (key,value)=field.split_once('=').unwrap_or((field,""));
            let slot=match key.trim().to_ascii_lowercase().as_str() {"profile"=>&mut profile,"level-idx"=>&mut level,"tier"=>&mut tier,_=>continue};
            let value=value.trim();
            if slot.is_some() || value.is_empty() || !value.bytes().all(|byte|byte.is_ascii_digit()) {return false;}
            let Ok(value)=value.parse::<u8>() else {return false;};*slot=Some(value);
        }
        let (profile,level,tier)=(profile.unwrap_or(0),level.unwrap_or(5),tier.unwrap_or(0));
        profile<=2 && level<=31 && !(24..=30).contains(&level) && tier<=1 && (tier==0 || level>=8)
            && self.level_idx<=level && u8::from(self.high_tier)<=tier
    }
}

/// Read the actual main-profile level/tier from a complete random-access unit.
/// The initial implementation accepts one operating point, without timing/HRD
/// or scalable-layer signaling. Those require explicit support, not
/// relabeling, frame dropping or an unbounded decoder configuration.
pub fn av1_main_profile(temporal_unit:&[u8])->Result<Av1MainProfile,MediaError> {
    validate_temporal_unit(temporal_unit,None,true)?.ok_or(MediaError::InvalidFrame)
}

#[derive(Default)]
enum Picture {#[default] None,Header,Tiles,Complete}

pub(crate) fn validate_temporal_unit(data:&[u8],expected:Option<Av1MainProfile>,keyframe:bool)->Result<Option<Av1MainProfile>,MediaError> {
    if data.is_empty() || data.len()>MAX_FRAME_BYTES {return Err(MediaError::InvalidFrame);}
    let mut offset=0;let mut count=0;let mut sequence=None;let mut picture=Picture::None;
    while offset<data.len() {
        count+=1;if count>256 {return Err(MediaError::InvalidFrame);}
        let header=data[offset];offset+=1;
        // Forbidden/reserved bits and absent sizes must never reach the native
        // packetizer's unchecked slice. Layer IDs are zero for this L1T1 path.
        if header&0x81!=0 || header&2==0 {return Err(MediaError::InvalidFrame);}
        let kind=(header>>3)&15;
        if header&4!=0 {
            if data.get(offset)!=Some(&0) || kind==1 {return Err(MediaError::UnsupportedCodec);}
            offset+=1;
        }
        let mut length=0u64;let mut size_bytes=0;
        loop {
            let byte=*data.get(offset).ok_or(MediaError::InvalidFrame)?;offset+=1;
            length|=u64::from(byte&127)<<(7*size_bytes);size_bytes+=1;
            if byte&128==0 {break;}
            if size_bytes==8 {return Err(MediaError::InvalidFrame);}
        }
        if length>MAX_FRAME_BYTES as u64 {return Err(MediaError::InvalidFrame);}
        let end=offset.checked_add(length as usize).ok_or(MediaError::InvalidFrame)?;
        let payload=data.get(offset..end).ok_or(MediaError::InvalidFrame)?;offset=end;
        match kind {
            1=>{
                if sequence.is_some() || !matches!(picture,Picture::None) || !keyframe {return Err(MediaError::InvalidFrame);}
                let observed=sequence_profile(payload)?;
                if expected.is_some_and(|profile|profile!=observed) {return Err(MediaError::InvalidFrame);}
                sequence=Some(observed);
            },
            2 if count==1 && payload.is_empty()=>{},
            3 if !payload.is_empty() && matches!(picture,Picture::None)=>picture=Picture::Header,
            4 if !payload.is_empty() && matches!(picture,Picture::Header|Picture::Tiles)=>picture=Picture::Tiles,
            6 if !payload.is_empty() && matches!(picture,Picture::None)=>picture=Picture::Complete,
            5 if !payload.is_empty()=>{},
            15=>{},
            _=>return Err(MediaError::InvalidFrame),
        }
    }
    if !matches!(picture,Picture::Tiles|Picture::Complete) || keyframe && sequence.is_none() {return Err(MediaError::InvalidFrame);}
    Ok(sequence)
}

fn sequence_profile(data:&[u8])->Result<Av1MainProfile,MediaError> {
    let mut bits=Bits {data,offset:0};
    if bits.read(3)?!=0 || bits.read(1)?!=0 || bits.read(1)?!=0 || bits.read(1)?!=0 {return Err(MediaError::UnsupportedCodec);}
    let initial_delay=bits.read(1)?!=0;
    if bits.read(5)?!=0 || bits.read(12)?!=0 {return Err(MediaError::UnsupportedCodec);}
    let level_idx=bits.read(5)? as u8;
    let high_tier=level_idx>7 && bits.read(1)?!=0;
    if initial_delay && bits.read(1)?!=0 {bits.read(4)?;}
    let profile=Av1MainProfile {level_idx,high_tier};profile.validate()?;Ok(profile)
}
struct Bits<'a> {data:&'a [u8],offset:usize}
impl Bits<'_> {
    fn read(&mut self,count:usize)->Result<u32,MediaError> {
        if self.offset+count>self.data.len()*8 {return Err(MediaError::InvalidFrame);}
        let mut value=0;
        for _ in 0..count {value=(value<<1)|u32::from((self.data[self.offset/8]>>(7-self.offset%8))&1);self.offset+=1;}
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key()->Vec<u8> {vec![0x12,0,0x0a,4,0,0,0,0,0x32,1,0]}

    #[test]
    fn av1_framing_requires_complete_sized_single_picture_units() {
        let profile=Av1MainProfile {level_idx:0,high_tier:false};
        assert_eq!(av1_main_profile(&key()).unwrap(),profile);
        assert_eq!(validate_temporal_unit(&[0x32,1,0],Some(profile),false).unwrap(),None);
        assert!(validate_temporal_unit(&[0x1a,1,0,0x22,1,0],Some(profile),false).is_ok());
        for data in [vec![],vec![0x32],vec![0x32,2,0],vec![0xb2,1,0],vec![0x33,1,0],vec![0x30,0],
            vec![0x32,0x80],vec![0x32,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0],
            vec![0x32,1,0,0x32,1,0],vec![0x32,1,0,0x12,0],vec![0x12,0],vec![0x1a,1,0],vec![0x22,1,0],vec![0x42,1,0]] {
            assert!(validate_temporal_unit(&data,Some(profile),false).is_err(),"bad OBU framing accepted: {data:?}");
        }
        assert!(validate_temporal_unit(&[0x32,1,0],Some(profile),true).is_err());
        assert!(validate_temporal_unit(&key(),Some(profile),false).is_err());
        assert!(validate_temporal_unit(&key(),Some(Av1MainProfile {level_idx:1,..profile}),true).is_err());
        let mut excessive=[0x7a,0].repeat(256);excessive.extend([0x32,1,0]);
        assert!(validate_temporal_unit(&excessive,Some(profile),false).is_err());
    }

    #[test]
    fn av1_profile_rejects_truncation_scalability_and_unbounded_levels() {
        for bit in [7,4,3,2] {
            let mut data=key();data[4]|=1<<bit;
            assert!(av1_main_profile(&data).is_err());
        }
        let mut delay=key();delay[4]|=2;assert!(av1_main_profile(&delay).is_ok());
        delay[7]|=4;assert!(av1_main_profile(&delay).is_err());
        let mut truncated=key();truncated[3]=1;truncated.drain(5..8);
        assert!(av1_main_profile(&truncated).is_err());
        for level_idx in [2,3,6,7,10,11,20,21,22,23,24,31] {assert!(Av1MainProfile {level_idx,high_tier:false}.validate().is_err());}
        assert!(Av1MainProfile {level_idx:5,high_tier:true}.validate().is_err());
        assert!(Av1MainProfile {level_idx:13,high_tier:true}.validate().is_ok());
    }

    #[cfg(feature="webrtc")]
    #[test]
    fn av1_receiver_limits_are_asymmetric_and_ambiguous_limits_fail() {
        let profile=Av1MainProfile {level_idx:8,high_tier:false};
        for answer in ["profile=0;level-idx=8;tier=0","profile=2;level-idx=13;tier=1","level-idx=31;tier=1;future=value;flag"] {assert!(profile.receiver_supports(answer));}
        for answer in ["","level-idx=5","level-idx=8;profile=3","level-idx=8;level-idx=13","level-idx=8;tier=2","level-idx=-1","level-idx=256","level-idx=24","level-idx=8;profile=","level-idx=8;tier=1;tier=0"] {assert!(!profile.receiver_supports(answer));}
        assert!(!Av1MainProfile {level_idx:8,high_tier:true}.receiver_supports("level-idx=8;tier=0"));
        assert!(Av1MainProfile {level_idx:5,high_tier:false}.receiver_supports(""));
    }
}

#[cfg(test)]
pub(crate) fn fixture_frames()->Vec<Av1Frame> {
    let data=include_bytes!("../tests/fixtures/av1-main-low-delay.ivf");
    assert_eq!(&data[..4],b"DKIF");assert_eq!(&data[8..12],b"AV01");
    let mut offset=32;let mut frames=Vec::new();
    while offset<data.len() {
        let size=u32::from_le_bytes(data[offset..offset+4].try_into().unwrap())as usize;
        let timestamp=u64::from_le_bytes(data[offset+4..offset+12].try_into().unwrap());offset+=12;
        frames.push(Av1Frame {temporal_unit:data[offset..offset+size].to_vec(),keyframe:timestamp.is_multiple_of(10),presentation_time_micros:timestamp as i64*100_000});offset+=size;
    }
    assert_eq!(frames.len(),20);frames
}

#[cfg(test)]
mod encoded_fixture {
    use super::*;
    #[test]
    fn av1_real_low_delay_fixture_has_matching_main_profile_and_complete_units() {
        let frames=fixture_frames();let profile=av1_main_profile(&frames[0].temporal_unit).unwrap();
        assert_eq!(profile,Av1MainProfile {level_idx:0,high_tier:false});
        for frame in frames {assert!(validate_temporal_unit(&frame.temporal_unit,Some(profile),frame.keyframe).is_ok());}
    }
}
