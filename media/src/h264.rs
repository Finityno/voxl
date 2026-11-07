//! Read only the SPS prefix through VUI colour/chroma-location signaling.
//! This is bounded qualification metadata, not a complete H.264 validator,
//! decoder capability, colour conversion, or a source of inferred defaults.
use crate::MediaError;

const MAX_SPS_BYTES:usize=16*1024;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct VideoSignal {
    pub video_format:u8,pub full_range:bool,
    /// Raw H.264 colour_primaries / transfer_characteristics / matrix_coefficients.
    /// Absence stays absent; unspecified/reserved numeric values stay numeric.
    pub colour_description:Option<[u8;3]>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SpsSignal {
    pub profile_idc:u8,pub level_idc:u8,
    pub video_signal:Option<VideoSignal>,
    /// Raw chroma_sample_loc_type_top_field / bottom_field, if explicitly present.
    pub chroma_location:Option<[u8;2]>,
}

struct Bits<'a> {bytes:&'a [u8],position:usize}
impl Bits<'_> {
    fn read(&mut self,count:usize)->Result<u32,MediaError> {
        if count>32 || self.position.checked_add(count).is_none_or(|end|end>self.bytes.len()*8) {return Err(MediaError::InvalidFrame);}
        let mut value=0;for _ in 0..count {value=(value<<1)|u32::from((self.bytes[self.position/8]>>(7-self.position%8))&1);self.position+=1;}Ok(value)
    }
    fn flag(&mut self)->Result<bool,MediaError> {Ok(self.read(1)?!=0)}
    fn ue(&mut self,maximum:u32)->Result<u32,MediaError> {
        let mut zeros=0;while !self.flag()? {zeros+=1;if zeros>31 {return Err(MediaError::InvalidFrame);}}
        let value=(1u64<<zeros)-1+u64::from(self.read(zeros)?);
        if value>u64::from(maximum) {return Err(MediaError::InvalidFrame);}Ok(value as u32)
    }
    fn se(&mut self)->Result<i64,MediaError> {let value=i64::from(self.ue(u32::MAX)?);Ok(if value&1==0 {-value/2} else {(value+1)/2})}
}

/// Takes one raw SPS NAL, including its NAL header and excluding Annex-B framing.
/// At most 16 KiB of encoded parameter bytes are copied; no image is allocated.
/// Parsing ends after chroma location: later VUI/HRD/trailing syntax is not checked.
/// Missing signaling is returned as None, never filled from a profile or geometry.
pub fn sps_signal(sps:&[u8])->Result<SpsSignal,MediaError> {
    if sps.len()<5 || sps.len()>MAX_SPS_BYTES || sps[0]&0x9f!=7 || sps[0]&0x60==0 {return Err(MediaError::InvalidFrame);}
    let mut rbsp=Vec::new();rbsp.try_reserve_exact(sps.len()-1).map_err(|_|MediaError::Backpressure)?;
    let mut zeros=0;let mut position=1;
    while position<sps.len() {
        let byte=sps[position];
        if zeros==2 {
            if byte==3 {if sps.get(position+1).is_none_or(|next|*next>3) {return Err(MediaError::InvalidFrame);}zeros=0;position+=1;continue;}
            if byte<=2 {return Err(MediaError::InvalidFrame);}
        }
        rbsp.push(byte);zeros=if byte==0 {zeros+1} else {0};position+=1;
    }
    let mut bits=Bits {bytes:&rbsp,position:0};let profile_idc=bits.read(8)? as u8;
    if bits.read(8)?&3!=0 {return Err(MediaError::InvalidFrame);}let level_idc=bits.read(8)? as u8;bits.ue(31)?;
    if matches!(profile_idc,100|110|122|244|44|83|86|118|128|138|139|134|135) {
        let chroma=bits.ue(3)?;if chroma==3 {bits.flag()?;}
        bits.ue(6)?;bits.ue(6)?;bits.flag()?;
        if bits.flag()? {
            for list in 0..if chroma==3 {12} else {8} {if bits.flag()? {
                let mut last=8;let mut next=8;
                for _ in 0..if list<6 {16} else {64} {
                    if next!=0 {let delta=bits.se()?;if !(-128..=127).contains(&delta) {return Err(MediaError::InvalidFrame);}next=(last+delta+256)%256;}
                    if next!=0 {last=next;}
                }
            }}
        }
    } else if !matches!(profile_idc,66|77|88) {return Err(MediaError::UnsupportedCodec);}
    bits.ue(12)?;
    match bits.ue(2)? {
        0=>{bits.ue(12)?;},
        1=>{bits.flag()?;bits.se()?;bits.se()?;let count=bits.ue(255)?;for _ in 0..count {bits.se()?;}},
        _=>{},
    }
    bits.ue(16)?;bits.flag()?;bits.ue(u32::MAX)?;bits.ue(u32::MAX)?;
    if !bits.flag()? {bits.flag()?;}bits.flag()?;
    if bits.flag()? {for _ in 0..4 {bits.ue(u32::MAX)?;}}
    let mut signal=SpsSignal {profile_idc,level_idc,video_signal:None,chroma_location:None};
    if !bits.flag()? {return Ok(signal);}
    if bits.flag()? {let ratio=bits.read(8)?;if ratio==255 {bits.read(16)?;bits.read(16)?;}}
    if bits.flag()? {bits.flag()?;}
    if bits.flag()? {
        let video_format=bits.read(3)? as u8;let full_range=bits.flag()?;
        let colour_description=if bits.flag()? {Some([bits.read(8)? as u8,bits.read(8)? as u8,bits.read(8)? as u8])} else {None};
        signal.video_signal=Some(VideoSignal {video_format,full_range,colour_description});
    }
    if bits.flag()? {signal.chroma_location=Some([bits.ue(5)? as u8,bits.ue(5)? as u8]);}Ok(signal)
}

#[cfg(test)]
mod tests;
