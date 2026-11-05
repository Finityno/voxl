//! Normalize only documented encoder framing. No byte-pattern codec guessing,
//! profile relabeling or dropping a partial reference access unit is permitted.
use crate::{EncodedFrame,MediaError,MAX_FRAME_BYTES,codec::VideoCodec,nal_units_to_annex_b};

const MAX_HEADER_BYTES:usize=64*1024;
fn start(data:&[u8],at:usize)->Option<usize> {
    if data.get(at..at+4)==Some(&[0,0,0,1]) {Some(4)}
    else if data.get(at..at+3)==Some(&[0,0,1]) {Some(3)} else {None}
}
fn annex(data:&[u8],codec:VideoCodec)->Result<Vec<u8>,MediaError> {
    if data.is_empty() || data.len()>MAX_FRAME_BYTES {return Err(MediaError::InvalidFrame);}
    if start(data,0).is_some() {Ok(data.to_vec())} else {nal_units_to_annex_b(data,4,codec).map(|value|value.0)}
}
#[derive(Default)]
struct Nals {parameters:u8,keyframe:bool,picture:bool}
fn inspect(data:&[u8],codec:VideoCodec)->Result<Nals,MediaError> {
    let mut found=Nals::default();let mut offset=0;let mut count=0;
    while offset<data.len() {
        count+=1;if count>256 {return Err(MediaError::InvalidFrame);}
        let length=start(data,offset).ok_or(MediaError::InvalidFrame)?;offset+=length;
        let mut end=offset;
        while end<data.len() && start(data,end).is_none() {end+=1;}
        let unit=data.get(offset..end).filter(|unit|!unit.is_empty()).ok_or(MediaError::InvalidFrame)?;
        match codec {
            VideoCodec::H264=>{
                let kind=unit[0]&31;
                if unit[0]&128!=0 || !(1..=23).contains(&kind) {return Err(MediaError::InvalidFrame);}
                found.picture|=(1..=5).contains(&kind);found.keyframe|=kind==5;
                found.parameters|=match kind {7=>1,8=>2,_=>0};
            },
            VideoCodec::Hevc=>{
                if unit.len()<2 || unit[0]&0x81!=0 || unit[1]&0xf8!=0 || unit[1]&7==0 {return Err(MediaError::InvalidFrame);}
                let kind=(unit[0]>>1)&63;found.picture|=kind<=31;found.keyframe|=(16..=21).contains(&kind);
                found.parameters|=match kind {32=>1,33=>2,34=>4,_=>0};
            },
            VideoCodec::Av1=>return Err(MediaError::UnsupportedCodec),
        }
        offset=end;
    }
    Ok(found)
}
fn append_units(data:&[u8],offset:&mut usize,count:usize,out:&mut Vec<u8>,codec:VideoCodec,kind:u8)->Result<(),MediaError> {
    if count>256 {return Err(MediaError::InvalidFrame);}
    for _ in 0..count {
        let size=data.get(*offset..*offset+2).ok_or(MediaError::InvalidFrame)?;*offset+=2;
        let size=usize::from(u16::from_be_bytes(size.try_into().unwrap()));
        if size==0 || out.len().saturating_add(4+size)>MAX_HEADER_BYTES {return Err(MediaError::InvalidFrame);}
        let unit=data.get(*offset..*offset+size).ok_or(MediaError::InvalidFrame)?;*offset+=size;
        if match codec {VideoCodec::H264=>unit[0]&31!=kind,VideoCodec::Hevc=>(unit[0]>>1)&63!=kind,VideoCodec::Av1=>true} {return Err(MediaError::InvalidFrame);}
        out.extend_from_slice(&[0,0,0,1]);out.extend_from_slice(unit);
    }
    Ok(())
}
pub(super) fn headers(data:&[u8],codec:VideoCodec)->Result<Vec<u8>,MediaError> {
    if data.is_empty() {return Ok(Vec::new());}
    if data.len()>MAX_HEADER_BYTES {return Err(MediaError::InvalidFrame);}
    let value=if start(data,0).is_some() {data.to_vec()}
    else if data[0]==1 {
        let mut value=Vec::new();
        match codec {
            VideoCodec::H264=>{
                if data.len()<7 || data[4]!=255 || data[5]&0xe0!=0xe0 {return Err(MediaError::InvalidFrame);}
                let mut at=6;append_units(data,&mut at,usize::from(data[5]&31),&mut value,codec,7)?;
                let count=*data.get(at).ok_or(MediaError::InvalidFrame)?;at+=1;append_units(data,&mut at,usize::from(count),&mut value,codec,8)?;
                // Baseline AVCDecoderConfigurationRecord has no profile extensions.
                if data[1]!=66 || at!=data.len() {return Err(MediaError::UnsupportedCodec);}
            },
            VideoCodec::Hevc=>{
                if data.len()<23 || data[21]&3!=3 {return Err(MediaError::InvalidFrame);}
                let mut at=23;
                for _ in 0..data[22] {
                    let array=data.get(at..at+3).ok_or(MediaError::InvalidFrame)?;at+=3;
                    if array[0]&0x40!=0 {return Err(MediaError::InvalidFrame);}
                    append_units(data,&mut at,usize::from(u16::from_be_bytes([array[1],array[2]])),&mut value,codec,array[0]&63)?;
                }
                if at!=data.len() {return Err(MediaError::InvalidFrame);}
            },
            VideoCodec::Av1=>return Err(MediaError::UnsupportedCodec),
        }
        value
    } else {annex(data,codec)?};
    let found=inspect(&value,codec)?;
    let complete=if codec==VideoCodec::H264 {3} else {7};
    if found.picture || found.parameters!=complete {return Err(MediaError::InvalidFrame);}
    Ok(value)
}
pub(super) fn output(data:&[u8],parameter_sets:&[u8],codec:VideoCodec,clean_point:bool,pts:i64)->Result<EncodedFrame,MediaError> {
    let mut data=annex(data,codec)?;let found=inspect(&data,codec)?;
    if !found.picture || clean_point && !found.keyframe {return Err(MediaError::InvalidFrame);}
    let complete=if codec==VideoCodec::H264 {3} else {7};
    if found.keyframe && found.parameters!=complete {
        // A partial in-band update cannot be completed with possibly stale
        // out-of-band parameters. Start a fresh encoder generation instead.
        if found.parameters!=0 {return Err(MediaError::InvalidFrame);}
        let prefix=headers(parameter_sets,codec)?;
        if prefix.is_empty() || prefix.len().saturating_add(data.len())>MAX_FRAME_BYTES {return Err(MediaError::InvalidFrame);}
        let mut combined=prefix;combined.append(&mut data);data=combined;
    }
    Ok(EncodedFrame {codec,annex_b:data,keyframe:found.keyframe,presentation_time_micros:pts})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h264()->Vec<u8> {vec![0,0,0,1,0x67,66,192,31,0,0,0,1,0x68,1]}
    #[test]
    fn windows_bitstream_restores_parameter_sets_without_relabeling_or_clock_changes() {
        let frame=output(&[0,0,0,2,0x65,7],&h264(),VideoCodec::H264,true,123).unwrap();
        assert_eq!(frame.presentation_time_micros,123);assert!(frame.keyframe);assert_eq!(&frame.annex_b[..h264().len()],h264());
        assert_eq!(crate::h264_profile_level_id(&frame.annex_b).unwrap(),[66,192,31]);
        assert!(!output(&[0,0,1,0x41,3],&[],VideoCodec::H264,false,124).unwrap().keyframe);
        assert!(output(&[0,0,1,0x41,3],&h264(),VideoCodec::H264,true,124).is_err());
        assert!(output(&[0,0,1,0x65,3],&[],VideoCodec::H264,true,124).is_err());
        assert!(output(&[0,0,1,0x67,66,192,31,0,0,1,0x65,3],&h264(),VideoCodec::H264,true,125).is_err());
    }
    #[test]
    fn windows_bitstream_parses_avcc_and_hvcc_headers_and_rejects_truncation() {
        let avcc=[1,66,192,31,255,225,0,4,0x67,66,192,31,1,0,2,0x68,1];assert_eq!(headers(&avcc,VideoCodec::H264).unwrap(),h264());
        for end in 0..avcc.len() {if end>0 {assert!(headers(&avcc[..end],VideoCodec::H264).is_err());}}
        let mut hvcc=vec![0;23];hvcc[0]=1;hvcc[21]=3;hvcc[22]=3;
        let mut expected=Vec::new();
        for kind in [32,33,34] {hvcc.extend([kind,0,1,0,2,kind<<1,1]);expected.extend([0,0,0,1,kind<<1,1]);}
        assert_eq!(headers(&hvcc,VideoCodec::Hevc).unwrap(),expected);
        let mut mismatched=hvcc.clone();mismatched[23]=33;
        assert!(headers(&mismatched,VideoCodec::Hevc).is_err());
        let frame=output(&[0,0,1,0x26,1,3],&hvcc,VideoCodec::Hevc,true,17).unwrap();assert!(frame.keyframe);
        assert!(output(&[0,0,1,0xa6,1],&hvcc,VideoCodec::Hevc,true,18).is_err());
        assert!(output(&[0,0,0,3,0x65],&h264(),VideoCodec::H264,true,0).is_err());
    }
}
