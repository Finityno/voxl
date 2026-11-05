//! Explicit SDR BGRA -> BT.709 limited NV12 scaling/conversion on D3D11.
//! Owns a bounded surface pool; encoder sample-release leases, not output
//! arrival, allow reuse. CPU codec fallback reads only converted NV12.
use super::{VideoEncoder,SurfaceLease,Nv12Color,Wake,checked,capture::{CaptureDevice,CapturedFrame}};
use crate::{EncoderConfig,MediaError};
use std::{mem::ManuallyDrop,ptr,rc::Rc,time::Duration};
use windows_core::{Interface,IUnknown};
use ::windows::Win32::{Foundation::{E_FAIL,RECT,WAIT_OBJECT_0},Graphics::{Direct3D11::*,Dxgi::Common::*},System::Threading::{CreateEventW,WaitForSingleObject}};

struct Slot {texture:ID3D11Texture2D,view:ID3D11VideoProcessorOutputView,lease:Option<SurfaceLease>}
/// Device/fence/conversion support is checked, never inferred from the encoder
/// registration. Requires an actual D3D11.4 GPU fence and VideoContext1 path.
pub struct Nv12Converter {
    video:ID3D11VideoDevice,context:ID3D11VideoContext1,enumerator:ID3D11VideoProcessorEnumerator,processor:ID3D11VideoProcessor,
    fence:ID3D11Fence,queue:ID3D11DeviceContext4,wake:Wake,value:u64,
    slots:Vec<Slot>,staging:Option<ID3D11Texture2D>,source:(u32,u32),config:EncoderConfig,failed:bool,idle:fn(),device:Rc<CaptureDevice>,
}
impl Nv12Converter {
    pub fn new(device:Rc<CaptureDevice>,source:(u32,u32),config:EncoderConfig,idle:fn())->Result<Self,MediaError> {
        config.validate()?;
        if source.0==0 || source.1==0 || source.0>16384 || source.1>16384 {return Err(MediaError::Configuration);}
        let video=checked("D3D11 video device",device.device.cast::<ID3D11VideoDevice>())?;
        let context=checked("D3D11 video color context",device.context.cast::<ID3D11VideoContext1>())?;
        let desc=D3D11_VIDEO_PROCESSOR_CONTENT_DESC {InputFrameFormat:D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,InputFrameRate:DXGI_RATIONAL {Numerator:config.fps,Denominator:1},InputWidth:source.0,InputHeight:source.1,OutputFrameRate:DXGI_RATIONAL {Numerator:config.fps,Denominator:1},OutputWidth:config.width,OutputHeight:config.height,Usage:D3D11_VIDEO_USAGE_PLAYBACK_NORMAL};
        // SAFETY: all native interfaces belong to one protected D3D11 device.
        unsafe {
            let enumerator=checked("NV12 conversion enumerator",video.CreateVideoProcessorEnumerator(&desc))?;
            let color=checked("NV12 conversion color support",enumerator.cast::<ID3D11VideoProcessorEnumerator1>())?;
            if !checked("BGRA to NV12 color conversion",color.CheckVideoProcessorFormatConversion(DXGI_FORMAT_B8G8R8A8_UNORM,DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,DXGI_FORMAT_NV12,DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709))?.as_bool() {return Err(MediaError::UnsupportedCodec);}
            let caps=checked("video conversion caps",enumerator.GetVideoProcessorCaps())?;let mut rate=None;
            if caps.RateConversionCapsCount>32 {return Err(MediaError::Configuration);}
            for index in 0..caps.RateConversionCapsCount {
                let caps=checked("video conversion history",enumerator.GetVideoProcessorRateConversionCaps(index))?;
                if caps.PastFrames==0 && caps.FutureFrames==0 {rate=Some(index);break;}
            }
            let processor=checked("create zero-history video processor",video.CreateVideoProcessor(&enumerator,rate.ok_or(MediaError::UnsupportedCodec)?))?;
            context.VideoProcessorSetStreamFrameFormat(&processor,0,D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            context.VideoProcessorSetStreamAutoProcessingMode(&processor,0,false);
            context.VideoProcessorSetStreamOutputRate(&processor,0,D3D11_VIDEO_PROCESSOR_OUTPUT_RATE_NORMAL,false,None);
            context.VideoProcessorSetStreamColorSpace1(&processor,0,DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709);
            context.VideoProcessorSetOutputColorSpace1(&processor,DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709);
            if context.VideoProcessorGetStreamColorSpace1(&processor,0)!=DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
                || context.VideoProcessorGetOutputColorSpace1(&processor)!=DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709 {return Err(MediaError::Configuration);}
            let input=RECT {left:0,top:0,right:source.0 as i32,bottom:source.1 as i32};let output=RECT {left:0,top:0,right:config.width as i32,bottom:config.height as i32};
            context.VideoProcessorSetStreamSourceRect(&processor,0,true,Some(&input));context.VideoProcessorSetStreamDestRect(&processor,0,true,Some(&output));context.VideoProcessorSetOutputTargetRect(&processor,true,Some(&output));
            let queue=checked("GPU completion queue",device.context.cast::<ID3D11DeviceContext4>())?;
            let modern=checked("GPU completion device",device.device.cast::<ID3D11Device5>())?;let mut fence=None;
            checked("create GPU completion fence",modern.CreateFence::<ID3D11Fence>(0,D3D11_FENCE_FLAG_NONE,&mut fence))?;
            let wake=Wake(checked("GPU completion event",CreateEventW(None,false,false,None))?);
            Ok(Self {video,context,enumerator,processor,fence:fence.ok_or(MediaError::Configuration)?,queue,wake,value:0,slots:Vec::with_capacity(3),staging:None,source,config,failed:false,idle,device})
        }
    }
    fn slot(&mut self)->Result<usize,MediaError> {
        for (index,slot) in self.slots.iter_mut().enumerate() {
            if slot.lease.as_ref().is_none_or(SurfaceLease::released) {slot.lease=None;return Ok(index);}
        }
        if self.slots.len()>=3 {return Err(MediaError::Backpressure);}
        let desc=self.description(false);let mut texture=None;let mut view=None;
        let output=D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {ViewDimension:D3D11_VPOV_DIMENSION_TEXTURE2D,..Default::default()};
        // SAFETY: newly allocated texture/view are owned by this bounded pool.
        unsafe {
            checked("allocate converted NV12 texture",self.device.device.CreateTexture2D(&desc,None,Some(&mut texture)))?;
            let texture=texture.ok_or(MediaError::Configuration)?;
            checked("NV12 processor output view",self.video.CreateVideoProcessorOutputView(&texture,&self.enumerator,&output,Some(&mut view)))?;
            self.slots.push(Slot {texture,view:view.ok_or(MediaError::Configuration)?,lease:None});
        }
        Ok(self.slots.len()-1)
    }
    fn description(&self,staging:bool)->D3D11_TEXTURE2D_DESC {
        D3D11_TEXTURE2D_DESC {Width:self.config.width,Height:self.config.height,MipLevels:1,ArraySize:1,Format:DXGI_FORMAT_NV12,SampleDesc:DXGI_SAMPLE_DESC {Count:1,Quality:0},Usage:if staging {D3D11_USAGE_STAGING} else {D3D11_USAGE_DEFAULT},BindFlags:if staging {0} else {D3D11_BIND_RENDER_TARGET.0 as u32},CPUAccessFlags:if staging {D3D11_CPU_ACCESS_READ.0 as u32} else {0},MiscFlags:0}
    }
    fn finish(&mut self)->Result<(),MediaError> {
        self.value=self.value.checked_add(1).ok_or(MediaError::Closed)?;
        // SAFETY: the fence/event and immediate context are exclusively owned.
        unsafe {
            checked("signal conversion fence",self.queue.Signal(&self.fence,self.value))?;
            self.device.context.Flush();checked("watch conversion fence",self.fence.SetEventOnCompletion(self.value,self.wake.0))?;
            let ready=WaitForSingleObject(self.wake.0,0);
            if ready!=WAIT_OBJECT_0 {
                (self.idle)();
                if WaitForSingleObject(self.wake.0,200)!=WAIT_OBJECT_0 {return Err(MediaError::Platform {operation:"GPU conversion deadline",code:E_FAIL.0});}
            }
            checked("conversion device health",self.device.device.GetDeviceRemovedReason())?;
            let completed=self.fence.GetCompletedValue();if completed==u64::MAX || completed<self.value {return Err(MediaError::Closed);}
        }
        Ok(())
    }
    /// Input must be immutable and all producer work ordered/completed before
    /// this call. The returned fence guarantees conversion/readback completion.
    unsafe fn render(&mut self,input:&ID3D11Texture2D,readback:bool)->Result<usize,MediaError> {
        if self.failed {return Err(MediaError::Closed);}
        let slot=self.slot()?;
        // SAFETY: caller retains immutable source; device identity, complete
        // extent/format and single-subresource shape are checked before use.
        unsafe {
            let mut desc=D3D11_TEXTURE2D_DESC::default();input.GetDesc(&mut desc);
            let owner=checked("conversion texture device",input.GetDevice())?;
            if desc.Format!=DXGI_FORMAT_B8G8R8A8_UNORM || desc.Width!=self.source.0 || desc.Height!=self.source.1 || desc.MipLevels!=1 || desc.ArraySize!=1 || desc.SampleDesc.Count!=1
                || checked("source device identity",owner.cast::<IUnknown>())?.as_raw()!=checked("conversion device identity",self.device.device.cast::<IUnknown>())?.as_raw() {return Err(MediaError::InvalidFrame);}
            let desc=D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {FourCC:0,ViewDimension:D3D11_VPIV_DIMENSION_TEXTURE2D,..Default::default()};let mut view=None;
            checked("BGRA processor input view",self.video.CreateVideoProcessorInputView(input,&self.enumerator,&desc,Some(&mut view)))?;
            let mut stream=D3D11_VIDEO_PROCESSOR_STREAM {Enable:true.into(),pInputSurface:ManuallyDrop::new(view),..Default::default()};
            let result=self.context.VideoProcessorBlt(&self.processor,&self.slots[slot].view,0,std::slice::from_ref(&stream));
            let _view=ManuallyDrop::take(&mut stream.pInputSurface);checked("BGRA to NV12 GPU conversion",result)?;
            if readback {
                if self.staging.is_none() {let mut staging=None;checked("NV12 staging texture",self.device.device.CreateTexture2D(&self.description(true),None,Some(&mut staging)))?;self.staging=staging;}
                self.device.context.CopyResource(self.staging.as_ref().ok_or(MediaError::Configuration)?,&self.slots[slot].texture);
            }
        }
        if let Err(error)=self.finish() {self.failed=true;return Err(error);}
        Ok(slot)
    }
    fn readback(&self)->Result<Vec<u8>,MediaError> {
        let texture=self.staging.as_ref().ok_or(MediaError::Configuration)?;let width=self.config.width as usize;let rows=self.config.height as usize*3/2;
        let mut map=D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the conversion/copy fence completed. Map is balanced by a
        // guard on every return; only width active bytes of each NV12 row copy.
        checked("map converted NV12",unsafe {self.device.context.Map(texture,0,D3D11_MAP_READ,0,Some(&mut map))})?;
        struct Mapping<'a>(&'a ID3D11DeviceContext,&'a ID3D11Texture2D);
        impl Drop for Mapping<'_> {fn drop(&mut self) {
            // SAFETY: one successful Map, for the same owned subresource.
            unsafe {self.0.Unmap(self.1,0);}
        }}
        let _mapping=Mapping(&self.device.context,texture);
        if map.pData.is_null() || (map.RowPitch as usize)<width || map.RowPitch>65536 {return Err(MediaError::InvalidFrame);}
        let mut data=vec![0;width*rows];
        for row in 0..rows {
            // SAFETY: D3D11 Map of this NV12 texture exposes height*3/2 rows at
            // RowPitch. The copied active width is within both checked spans.
            unsafe {ptr::copy_nonoverlapping(map.pData.cast::<u8>().add(row*map.RowPitch as usize),data.as_mut_ptr().add(row*width),width);}
        }
        Ok(data)
    }
    /// Consume one authorized capture frame into the configured encoder. No
    /// native frame is checked back into WGC until GPU source reads complete.
    pub fn encode(&mut self,frame:CapturedFrame<'_>,encoder:&mut VideoEncoder,force:bool)->Result<(),MediaError> {
        frame.validate()?;
        if encoder.config.width!=self.config.width || encoder.config.height!=self.config.height || encoder.observation.color_requested!=Nv12Color::Bt709Limited {return Err(MediaError::Configuration);}
        if !encoder.wait_ready(Duration::from_millis(200))? {return Err(MediaError::Backpressure);}
        let readback=encoder.device.is_none();
        // SAFETY: WGC retains immutable pixels until frame Drop. Our GPU fence
        // completes all reads before returning it. On failure stop the producer.
        let slot=match unsafe {self.render(&frame.texture,readback)} {Ok(slot)=>slot,Err(error)=>{frame.capture_close();self.failed=true;encoder.close();return Err(error)}};
        frame.validate()?;
        if readback {encoder.encode_nv12(&self.readback()?,self.config.width as usize,frame.presentation_time_micros(),force)} else {
            // SAFETY: this slot is exclusively owned and never written until its
            // tracked native sample-release signal. The producer fence completed.
            match unsafe {encoder.encode_surface(&self.slots[slot].texture,0,frame.presentation_time_micros(),force)} {
                Ok(lease)=>{self.slots[slot].lease=Some(lease);Ok(())},
                Err(error)=>{self.failed=true;encoder.close();Err(error)},
            }
        }
    }
    /// Synthetic qualification only. No display is selected/captured. Produces
    /// tightly packed BT.709-limited NV12 for independent numeric/decode oracles.
    ///
    /// # Safety
    /// The input GPU texture must be immutable until this method returns, with
    /// producer work explicitly ordered/completed on this protected device.
    pub unsafe fn fixture_nv12(&mut self,input:&ID3D11Texture2D)->Result<Vec<u8>,MediaError> {
        // SAFETY: caller supplies the immutable source contract above.
        unsafe {self.render(input,true)?;}self.readback()
    }
}
