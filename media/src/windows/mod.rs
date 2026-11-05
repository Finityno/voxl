//! Thread-owned Media Foundation encoders. D3D11 surfaces stay on the GPU when
//! the selected hardware MFT accepts a device manager. System-memory NV12 is
//! available for explicit, bounded H.264 CPU fallback. No capture, process
//! runtime, signaling endpoint or long-lived Rust thread is created here.

mod bitstream;
use crate::{EncodedFrame,EncoderConfig,EncoderPreference,MediaError,MAX_FRAME_BYTES,av1::{Av1Frame,av1_main_profile,validate_temporal_unit},codec::VideoCodec};
use std::{marker::PhantomData,mem::ManuallyDrop,ptr,rc::Rc,sync::{Arc,atomic::{AtomicBool,AtomicI32,AtomicU32,Ordering}},time::{Duration,Instant}};
use windows_core::{implement,Interface,IUnknown,IUnknownImpl,Ref,GUID,HRESULT};
use ::windows::Win32::{Foundation::{CloseHandle,E_FAIL,E_NOTIMPL,E_POINTER,HANDLE,WAIT_FAILED,WAIT_OBJECT_0,WAIT_TIMEOUT},Graphics::{Direct3D11::{ID3D11Device,ID3D11Texture2D,ID3D11Multithread,D3D11_TEXTURE2D_DESC},Dxgi::Common::DXGI_FORMAT_NV12},Media::MediaFoundation::*,System::{Com::{CoInitializeEx,CoTaskMemFree,CoUninitialize,COINIT_MULTITHREADED},Threading::{CreateEventW,SetEvent,WaitForSingleObject},Variant::VARIANT}};

fn platform(operation:&'static str,error:windows_core::Error)->MediaError {MediaError::Platform {operation,code:error.code().0}}
fn checked<T>(operation:&'static str,result:windows_core::Result<T>)->Result<T,MediaError> {result.map_err(|error|platform(operation,error))}

/// Requests describe the NV12 pixels, never an inference from frame dimensions.
/// Capture/conversion must produce this colorimetry before encoding. HDR/10-bit
/// input requires a separate conversion/format path and is not relabeled here.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Nv12Color {Bt709Limited,Bt709Full,Bt601Limited,Bt601Full}
impl Nv12Color {
    fn set(self,media:&IMFMediaType)->windows_core::Result<()> {
        let matrix=if matches!(self,Self::Bt709Limited|Self::Bt709Full) {MFVideoTransferMatrix_BT709} else {MFVideoTransferMatrix_BT601};
        let primaries=if matches!(self,Self::Bt709Limited|Self::Bt709Full) {MFVideoPrimaries_BT709} else {MFVideoPrimaries_SMPTE170M};
        let range=if matches!(self,Self::Bt709Full|Self::Bt601Full) {MFNominalRange_0_255} else {MFNominalRange_16_235};
        // SAFETY: setters operate on the live, thread-owned media type.
        unsafe {media.SetUINT32(&MF_MT_VIDEO_PRIMARIES,primaries.0 as u32)?;media.SetUINT32(&MF_MT_TRANSFER_FUNCTION,MFVideoTransFunc_709.0 as u32)?;media.SetUINT32(&MF_MT_YUV_MATRIX,matrix.0 as u32)?;media.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE,range.0 as u32)}
    }
}
/// Registration and accepted input path are observed separately from active GPU
/// work, driver-internal copies, power and measured encode latency.
#[derive(Clone,Copy,Debug)]
pub struct EncoderObservation {pub codec:VideoCodec,pub hardware_registration:bool,pub device_input:bool,pub asynchronous:bool,pub width:u32,pub height:u32,pub fps:u32,pub bitrate_kbps:u32,pub color_requested:Nv12Color}
#[derive(Debug)]
pub enum EncodedOutput {Nal(EncodedFrame),Av1(Av1Frame)}

struct Context(PhantomData<Rc<()>>);
impl Context {
    fn new()->Result<Self,MediaError> {
        // SAFETY: the caller's dedicated worker initializes one MTA reference;
        // the !Send context balances it on this same thread after COM teardown.
        unsafe {
            checked("initialize encoder MTA",CoInitializeEx(None,COINIT_MULTITHREADED).ok())?;
            if let Err(error)=MFStartup(MF_VERSION,MFSTARTUP_FULL) {CoUninitialize();return Err(platform("start Media Foundation",error));}
        }
        Ok(Self(PhantomData))
    }
}
impl Drop for Context {fn drop(&mut self) {
    // SAFETY: all encoder COM fields are declared before this context and have
    // been shut down/released; both calls balance successful construction.
    unsafe {let _=MFShutdown();CoUninitialize();}
}}
struct Wake(HANDLE);
// SAFETY: Win32 event handles permit concurrent SetEvent/wait. Arc ownership
// keeps the handle open through every native callback and waiter.
unsafe impl Send for Wake {}
// SAFETY: the same event is a kernel synchronization object, not a COM object.
unsafe impl Sync for Wake {}
impl Drop for Wake {fn drop(&mut self) {
    // SAFETY: this is the unique final owner of a CreateEventW handle.
    unsafe {let _=CloseHandle(self.0);}
}}
struct Signals {wake:Wake,closed:AtomicBool,need_input:AtomicU32,outputs:AtomicU32,error:AtomicI32}
impl Signals {
    fn new()->Result<Arc<Self>,MediaError> {
        // SAFETY: an unnamed auto-reset event with no inherited/security state.
        let wake=checked("create encoder wake",unsafe {CreateEventW(None,false,false,None)})?;
        Ok(Arc::new(Self {wake:Wake(wake),closed:AtomicBool::new(false),need_input:AtomicU32::new(0),outputs:AtomicU32::new(0),error:AtomicI32::new(0)}))
    }
    fn notify(&self) {
        // SAFETY: Arc retains the event through this native callback.
        unsafe {let _=SetEvent(self.wake.0);}
    }
    fn failed(&self,code:HRESULT) {self.error.store(code.0,Ordering::Release);self.notify();}
    fn check(&self)->Result<(),MediaError> {
        let code=self.error.load(Ordering::Acquire);
        if code<0 {Err(MediaError::Platform {operation:"encoder callback",code})} else if self.closed.load(Ordering::Acquire) {Err(MediaError::Closed)} else {Ok(())}
    }
}
#[implement(IMFAsyncCallback)]
struct Callback {signals:Arc<Signals>}
impl IMFAsyncCallback_Impl for Callback_Impl {
    fn GetParameters(&self,_flags:*mut u32,_queue:*mut u32)->windows_core::Result<()> {Err(E_NOTIMPL.into())}
    fn Invoke(&self,result:Ref<'_,IMFAsyncResult>)->windows_core::Result<()> {
        if self.signals.closed.load(Ordering::Acquire) {return Ok(());}
        let outcome=(||->windows_core::Result<()> {
            let result=result.as_ref().ok_or_else(||windows_core::Error::from(E_POINTER))?;
            // SAFETY: BeginGetEvent stores this live generator as its COM state.
            // The callback uses atomics/COM references only, without allocating
            // frame data or borrowing the owning encoder/embedding application.
            unsafe {
                let generator=result.GetState()?.cast::<IMFMediaEventGenerator>()?;
                let event=generator.EndGetEvent(result)?;event.GetStatus()?.ok()?;
                let kind=event.GetType()?;
                if kind==METransformNeedInput.0 as u32 && self.signals.need_input.fetch_add(1,Ordering::AcqRel)>=16 {return Err(E_FAIL.into());}
                if kind==METransformHaveOutput.0 as u32 && self.signals.outputs.fetch_add(1,Ordering::AcqRel)>=4 {return Err(E_FAIL.into());}
                self.signals.notify();
                if !self.signals.closed.load(Ordering::Acquire) {
                    let callback:IMFAsyncCallback=self.to_interface();generator.BeginGetEvent(&callback,&generator)?;
                }
            }
            Ok(())
        })();
        if let Err(error)=outcome {self.signals.failed(error.code());}
        Ok(())
    }
}
struct Activations {values:*mut Option<IMFActivate>,count:usize}
impl Activations {
    fn enumerate(codec:VideoCodec,hardware:bool)->Result<Self,MediaError> {
        let output=MFT_REGISTER_TYPE_INFO {guidMajorType:MFMediaType_Video,guidSubtype:subtype(codec)};
        let flags=(if hardware {MFT_ENUM_FLAG_HARDWARE} else {MFT_ENUM_FLAG_SYNCMFT})|MFT_ENUM_FLAG_SORTANDFILTER;
        let mut values=ptr::null_mut();let mut count=0;
        // SAFETY: outputs point to writable locals. The CoTaskMem array owns
        // one COM reference per entry and is released by Activations::drop.
        checked("enumerate video encoders",unsafe {MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,flags,None,Some(&output),&mut values,&mut count)})?;
        if count>0 && values.is_null() {return Err(MediaError::Configuration);}
        Ok(Self {values,count:count as usize})
    }
    fn get(&self,index:usize)->Option<IMFActivate> {
        if index>=self.count {return None;}
        // SAFETY: the native array has count initialized Option<COM> entries.
        unsafe {(*self.values.add(index)).clone()}
    }
}
impl Drop for Activations {fn drop(&mut self) {
    // SAFETY: release each owned interface before freeing its native array.
    unsafe {for index in 0..self.count {ptr::drop_in_place(self.values.add(index));}CoTaskMemFree(Some(self.values.cast()));}
}}
struct SetupGuard {activation:Option<IMFActivate>,shutdown:Option<IMFShutdown>,signals:Option<Arc<Signals>>}
impl Drop for SetupGuard {fn drop(&mut self) {
    if let Some(signals)=&self.signals {signals.closed.store(true,Ordering::Release);signals.notify();}
    // SAFETY: unsuccessful setup must break the native callback/event cycle
    // before returning its MTA context to the candidate enumeration loop.
    unsafe {
        if let Some(shutdown)=&self.shutdown {let _=shutdown.Shutdown();}
        if let Some(activation)=&self.activation {let _=activation.ShutdownObject();}
    }
}}
/// A surface may be written again only after this native sample-release signal,
/// including after encoder close. Receiving an encoded frame alone is not proof
/// that the driver has stopped retaining the input texture.
pub struct SurfaceLease(Arc<AtomicBool>);
impl SurfaceLease {pub fn released(&self)->bool {self.0.load(Ordering::Acquire)}}
#[implement(IMFAsyncCallback)]
struct SampleReleased {released:Arc<AtomicBool>,signals:Arc<Signals>}
impl IMFAsyncCallback_Impl for SampleReleased_Impl {
    fn GetParameters(&self,_flags:*mut u32,_queue:*mut u32)->windows_core::Result<()> {Err(E_NOTIMPL.into())}
    fn Invoke(&self,_result:Ref<'_,IMFAsyncResult>)->windows_core::Result<()> {
        self.released.store(true,Ordering::Release);self.signals.notify();Ok(())
    }
}
#[derive(Clone,Copy)]
enum Flight {Idle,Submitted(i64),Closed}

/// Own this encoder on one dedicated MTA worker. Its callbacks never run capture
/// or application code. One accepted input remains in flight until output is
/// consumed, so hardware buffering cannot become an unbounded latency queue.
pub struct VideoEncoder {
    transform:IMFTransform,activation:IMFActivate,api:ICodecAPI,
    _callback:Option<IMFAsyncCallback>,shutdown:Option<IMFShutdown>,signals:Arc<Signals>,
    _manager:Option<IMFDXGIDeviceManager>,device:Option<ID3D11Device>,surfaces:Vec<Arc<AtomicBool>>,
    input_id:u32,output_id:u32,info:MFT_OUTPUT_STREAM_INFO,
    config:EncoderConfig,observation:EncoderObservation,flight:Flight,last_pts:Option<i64>,headers:Vec<u8>,idle:fn(),
    // Last: Media Foundation and COM stop only after native references release.
    _context:Context,
}
impl VideoEncoder {
    pub fn new(config:EncoderConfig,color:Nv12Color,idle:fn())->Result<Self,MediaError> {Self::open(config,color,None,idle)}
    /// The dedicated capture worker must enable D3D11 multithread protection
    /// before sharing its device with the asynchronous encoder. This method
    /// checks that contract and does not mutate the caller's device settings.
    pub fn with_device(config:EncoderConfig,color:Nv12Color,device:&ID3D11Device,idle:fn())->Result<Self,MediaError> {Self::open(config,color,Some(device.clone()),idle)}
    fn open(config:EncoderConfig,color:Nv12Color,device:Option<ID3D11Device>,idle:fn())->Result<Self,MediaError> {
        config.validate()?;let mut context=Some(Context::new()?);let mut last=MediaError::UnsupportedCodec;
        for hardware in [true,false] {
            if config.preference==EncoderPreference::RequireHardware && !hardware || config.preference==EncoderPreference::Software && hardware {continue;}
            if !hardware && (config.codec!=VideoCodec::H264 || config.width>1280 || config.height>720 || config.fps>30 || device.is_some()) {continue;}
            let activations=Activations::enumerate(config.codec,hardware)?;
            for index in 0..activations.count.min(128) {
                let Some(activation)=activations.get(index) else {continue;};
                match Self::configure(config,color,device.clone(),idle,hardware,activation,context.take().unwrap()) {
                    Ok(value)=>return Ok(value),
                    Err((error,returned))=>{last=error;context=Some(returned);},
                }
            }
        }
        Err(last)
    }
    #[allow(clippy::too_many_arguments)]
    fn configure(config:EncoderConfig,color:Nv12Color,device:Option<ID3D11Device>,idle:fn(),hardware:bool,activation:IMFActivate,context:Context)->Result<Self,(MediaError,Context)> {
        let mut cleanup=SetupGuard {activation:Some(activation.clone()),shutdown:None,signals:None};
        let configured=(||->Result<_,MediaError> {
            // SAFETY: the activation and every resulting COM interface stay on
            // this MTA. Native media objects hold their own input/output refs.
            unsafe {
                let transform:IMFTransform=checked("activate video encoder",activation.ActivateObject())?;
                let attributes=checked("encoder attributes",transform.GetAttributes())?;
                let asynchronous=attributes.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0)!=0;
                if hardware && !asynchronous {return Err(MediaError::Configuration);}
                if asynchronous {checked("unlock asynchronous encoder",attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK,1))?;}
                let api=checked("encoder codec API",transform.cast::<ICodecAPI>())?;
                checked("low latency encoder",api.SetValue(&CODECAPI_AVLowLatencyMode,&VARIANT::from(true)))?;
                checked("disable encoder B frames",api.SetValue(&CODECAPI_AVEncMPVDefaultBPictureCount,&VARIANT::from(0u32)))?;
                checked("encoder CBR",api.SetValue(&CODECAPI_AVEncCommonRateControlMode,&VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32)))?;
                checked("encoder bitrate",api.SetValue(&CODECAPI_AVEncCommonMeanBitRate,&VARIANT::from(config.bitrate_kbps*1000)))?;
                checked("encoder GOP",api.SetValue(&CODECAPI_AVEncMPVGOPSize,&VARIANT::from(config.fps*2)))?;
                if !hardware {checked("software encoder threads",api.SetValue(&CODECAPI_AVEncNumWorkerThreads,&VARIANT::from(2u32)))?;}
                let mut input_count=0;let mut output_count=0;checked("encoder stream count",transform.GetStreamCount(&mut input_count,&mut output_count))?;
                if input_count!=1 || output_count!=1 {return Err(MediaError::Configuration);}
                let mut inputs=[0];let mut outputs=[0];
                if let Err(error)=transform.GetStreamIDs(&mut inputs,&mut outputs) {if error.code()!=E_NOTIMPL {return Err(platform("encoder stream IDs",error));}}
                let manager=if let Some(device)=&device {
                    if !hardware || attributes.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0)!=1 {return Err(MediaError::UnsupportedCodec);}
                    let immediate=checked("encoder device context",device.GetImmediateContext())?;
                    let protection=checked("encoder device threading",immediate.cast::<ID3D11Multithread>())?;
                    if !protection.GetMultithreadProtected().as_bool() {return Err(MediaError::Configuration);}
                    let mut token=0;let mut manager=None;checked("create DXGI manager",MFCreateDXGIDeviceManager(&mut token,&mut manager))?;
                    let manager=manager.ok_or(MediaError::Configuration)?;checked("reset DXGI manager",manager.ResetDevice(device,token))?;
                    checked("attach DXGI manager",transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER,manager.as_raw()as usize))?;Some(manager)
                } else {None};
                let output=media_type(config,color,false)?;checked("set encoder output",transform.SetOutputType(outputs[0],&output,0))?;
                let input=media_type(config,color,true)?;checked("set encoder input",transform.SetInputType(inputs[0],&input,0))?;
                let info=checked("encoder output size",transform.GetOutputStreamInfo(outputs[0]))?;check_info(&info)?;
                let current=checked("encoder output format",transform.GetOutputCurrentType(outputs[0]))?;
                verify_type(&current,config,color)?;
                let headers=sequence_header(&current)?;
                let signals=Signals::new()?;
                cleanup.signals=Some(Arc::clone(&signals));
                let (callback,shutdown)=if asynchronous {
                    let shutdown=checked("asynchronous encoder shutdown",transform.cast::<IMFShutdown>())?;
                    cleanup.shutdown=Some(shutdown.clone());
                    let generator=checked("encoder event source",transform.cast::<IMFMediaEventGenerator>())?;
                    let callback:IMFAsyncCallback=Callback {signals:Arc::clone(&signals)}.into();
                    checked("listen for encoder events",generator.BeginGetEvent(&callback,&generator))?;(Some(callback),Some(shutdown))
                } else {(None,None)};
                checked("begin encoder streaming",transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,0))?;
                checked("start encoder stream",transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM,0))?;
                Ok((transform,api,callback,shutdown,signals,manager,inputs[0],outputs[0],info,headers,asynchronous))
            }
        })();
        match configured {
            Ok((transform,api,callback,shutdown,signals,manager,input_id,output_id,info,headers,asynchronous))=>{
                cleanup.activation.take();cleanup.shutdown.take();cleanup.signals.take();
                Ok(Self {transform,activation,api,_callback:callback,shutdown,signals,_manager:manager,device:device.clone(),surfaces:Vec::with_capacity(3),input_id,output_id,info,config,observation:EncoderObservation {codec:config.codec,hardware_registration:hardware,device_input:device.is_some(),asynchronous,width:config.width,height:config.height,fps:config.fps,bitrate_kbps:config.bitrate_kbps,color_requested:color},flight:Flight::Idle,last_pts:None,headers,idle,_context:context})
            },
            Err(error)=>{drop(cleanup);Err((error,context))},
        }
    }
    pub fn observation(&self)->EncoderObservation {self.observation}
    /// Wait for an asynchronous input credit without submitting a placeholder
    /// frame. The caller still owns capture pacing and any overall watchdog.
    pub fn wait_ready(&mut self,timeout:Duration)->Result<bool,MediaError> {
        let deadline=Instant::now()+timeout.min(Duration::from_millis(200));
        loop {
            self.signals.check()?;
            match self.flight {Flight::Closed=>return Err(MediaError::Closed),Flight::Submitted(_)=>return Err(MediaError::Backpressure),Flight::Idle=>{}}
            if !self.observation.asynchronous || self.signals.need_input.load(Ordering::Acquire)>0 {return Ok(true);}
            if !self.wait_signal(deadline)? {return Ok(false);}
        }
    }
    fn available(&self,pts:i64)->Result<(),MediaError> {
        self.signals.check()?;
        match self.flight {Flight::Closed=>return Err(MediaError::Closed),Flight::Submitted(_)=>return Err(MediaError::Backpressure),Flight::Idle=>{}}
        if pts<0 || pts>i64::MAX/10 || self.last_pts.is_some_and(|last|pts<=last) {return Err(MediaError::InvalidFrame);}
        if self.observation.asynchronous && self.signals.need_input.load(Ordering::Acquire)==0 {return Err(MediaError::Backpressure);}
        Ok(())
    }
    /// Copy immutable system-memory NV12 into an MFT-owned buffer. No CPU RGB
    /// conversion is hidden in this method; stride and both planes are checked.
    pub fn encode_nv12(&mut self,data:&[u8],stride:usize,pts:i64,force_keyframe:bool)->Result<(),MediaError> {
        self.available(pts)?;
        if self.device.is_some() {return Err(MediaError::Configuration);}
        let width=self.config.width as usize;let rows=self.config.height as usize*3/2;
        if stride<width || stride>16384 || stride.checked_mul(rows)!=Some(data.len()) {return Err(MediaError::InvalidFrame);}
        // SAFETY: MF owns the destination allocation. Its locked span is checked
        // before any row copy, and the lock guard releases it on every return.
        let buffer=checked("allocate NV12 sample",unsafe {MFCreateMemoryBuffer((width*rows)as u32)})?;
        {
            let locked=Locked::new(&buffer)?;
            if locked.capacity<width*rows {return Err(MediaError::InvalidFrame);}
            for row in 0..rows {
                // SAFETY: both row spans are disjoint, live, and in bounds.
                unsafe {ptr::copy_nonoverlapping(data.as_ptr().add(row*stride),locked.data.add(row*width),width);}
            }
        }
        // SAFETY: the just-written buffer holds exactly width*rows bytes.
        checked("set NV12 sample length",unsafe {buffer.SetCurrentLength((width*rows)as u32)})?;
        self.submit(buffer,pts,force_keyframe,None)
    }
    /// Submit a device-owned NV12 texture without CPU readback.
    ///
    /// # Safety
    /// Pixels must remain immutable until the returned lease reports released,
    /// even if the encoder has closed. No other D3D context may mutate them while
    /// retained. Use the device supplied at construction. On an error, close and
    /// destroy this encoder before considering the texture for reuse. Complete
    /// or explicitly synchronize producer GPU work before submitting the texture.
    pub unsafe fn encode_surface(&mut self,texture:&ID3D11Texture2D,subresource:u32,pts:i64,force_keyframe:bool)->Result<SurfaceLease,MediaError> {
        self.surfaces.retain(|lease|!lease.load(Ordering::Acquire));
        if self.surfaces.len()>=3 {return Err(MediaError::Backpressure);}
        self.available(pts)?;let device=self.device.as_ref().ok_or(MediaError::Configuration)?;
        // SAFETY: the caller owns a live texture with the stated immutability;
        // the DXGI media buffer takes its own retained resource reference.
        unsafe {
            let mut desc=D3D11_TEXTURE2D_DESC::default();texture.GetDesc(&mut desc);
            let owner=checked("texture device",texture.GetDevice())?;
            if desc.Format!=DXGI_FORMAT_NV12 || desc.Width!=self.config.width || desc.Height!=self.config.height || desc.MipLevels!=1 || subresource>=desc.ArraySize
                || owner.cast::<IUnknown>().map_err(|e|platform("texture identity",e))?.as_raw()!=device.cast::<IUnknown>().map_err(|e|platform("encoder device identity",e))?.as_raw() {return Err(MediaError::InvalidFrame);}
            let buffer=checked("retain encoder texture",MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID,texture,subresource,false))?;
            let released=Arc::new(AtomicBool::new(false));
            self.submit(buffer,pts,force_keyframe,Some(Arc::clone(&released)))?;
            self.surfaces.push(Arc::clone(&released));Ok(SurfaceLease(released))
        }
    }
    fn submit(&mut self,buffer:IMFMediaBuffer,pts:i64,force:bool,released:Option<Arc<AtomicBool>>)->Result<(),MediaError> {
        // SAFETY: all native objects belong to this encoder MTA, and the sample
        // retains its buffer until the transform releases the accepted input.
        let result=unsafe {
            (||->windows_core::Result<()> {
                if force {self.api.SetValue(&CODECAPI_AVEncVideoForceKeyFrame,&VARIANT::from(1u32))?;}
                let sample=if let Some(released)=released {
                    let tracked=MFCreateTrackedSample()?;
                    let callback:IMFAsyncCallback=SampleReleased {released,signals:Arc::clone(&self.signals)}.into();
                    tracked.SetAllocator(&callback,None)?;tracked.cast::<IMFSample>()?
                } else {MFCreateSample()?};
                sample.AddBuffer(&buffer)?;sample.SetSampleTime(pts*10)?;sample.SetSampleDuration(10_000_000/i64::from(self.config.fps))?;
                if self.observation.asynchronous {self.signals.need_input.fetch_sub(1,Ordering::AcqRel);}
                self.transform.ProcessInput(self.input_id,&sample,0)
            })()
        };
        match result {
            Ok(())=>{self.flight=Flight::Submitted(pts);self.last_pts=Some(pts);Ok(())},
            Err(error)=>{self.close();Err(platform("submit encoder frame",error))},
        }
    }
    pub fn take_output(&mut self)->Result<Option<EncodedOutput>,MediaError> {
        self.signals.check()?;let pts=match self.flight {Flight::Idle=>return Ok(None),Flight::Closed=>return Err(MediaError::Closed),Flight::Submitted(pts)=>pts};
        if self.observation.asynchronous {
            if self.signals.outputs.load(Ordering::Acquire)==0 {return Ok(None);}
            // Only the owning encoder decrements credits; callbacks only add.
            self.signals.outputs.fetch_sub(1,Ordering::AcqRel);
        }
        let result=self.output_sample().and_then(|sample|self.decode_output(sample,pts));
        match result {
            Ok(output)=>{self.flight=Flight::Idle;Ok(Some(output))},
            Err(error)=>{self.close();Err(error)},
        }
    }
    fn output_sample(&mut self)->Result<IMFSample,MediaError> {
            // SAFETY: one output stream and one pending input are owned here.
            // ManuallyDrop COM outputs are explicitly taken on success/error.
            let result=unsafe {
                let sample=if self.info.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32!=0 {None} else {
                    let sample=checked("allocate output sample",MFCreateSample())?;
                    let alignment=self.info.cbAlignment.max(1)-1;
                    let buffer=checked("allocate output buffer",MFCreateAlignedMemoryBuffer(self.info.cbSize,alignment))?;
                    checked("attach output buffer",sample.AddBuffer(&buffer))?;Some(sample)
                };
                let mut output=[MFT_OUTPUT_DATA_BUFFER {dwStreamID:self.output_id,pSample:ManuallyDrop::new(sample),dwStatus:0,pEvents:ManuallyDrop::new(None)}];let mut status=0;
                let result=self.transform.ProcessOutput(0,&mut output,&mut status);
                let sample=ManuallyDrop::take(&mut output[0].pSample);let _events=ManuallyDrop::take(&mut output[0].pEvents);
                result.and_then(|()|if output[0].dwStatus!=0 || status!=0 {Err(E_FAIL.into())} else {Ok(sample)})
            };
            match result {
                Ok(Some(sample))=>return Ok(sample),
                Ok(None)=>return Err(MediaError::InvalidFrame),
                Err(error)=>return Err(platform("take encoder output",error)),
            }
    }
    fn decode_output(&self,sample:IMFSample,pts:i64)->Result<EncodedOutput,MediaError> {
        // SAFETY: the returned native sample and contiguous buffer are live.
        let (buffer,clean)=unsafe {
            if checked("encoded capture timestamp",sample.GetSampleTime())?!=pts*10 {return Err(MediaError::InvalidFrame);}
            (checked("encoded bytes",sample.ConvertToContiguousBuffer())?,sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0)!=0)
        };
        let locked=Locked::new(&buffer)?;
        if locked.length==0 || locked.length>MAX_FRAME_BYTES || locked.length>locked.capacity {return Err(MediaError::InvalidFrame);}
        // SAFETY: the validated locked span stays immutable through parsing.
        let data=unsafe {std::slice::from_raw_parts(locked.data,locked.length)};
        if self.config.codec==VideoCodec::Av1 {
            let mut unit=data.to_vec();
            if clean && av1_main_profile(&unit).is_err() {
                let header=if self.headers.first()==Some(&0x81) {self.headers.get(4..).ok_or(MediaError::InvalidFrame)?} else {&self.headers};
                if header.is_empty() || header.len().saturating_add(unit.len())>MAX_FRAME_BYTES {return Err(MediaError::InvalidFrame);}
                let mut complete=header.to_vec();complete.append(&mut unit);unit=complete;
            }
            validate_temporal_unit(&unit,None,clean)?;
            Ok(EncodedOutput::Av1(Av1Frame {temporal_unit:unit,keyframe:clean,presentation_time_micros:pts}))
        } else {bitstream::output(data,&self.headers,self.config.codec,clean,pts).map(EncodedOutput::Nal)}
    }
    /// Wait on native events rather than polling every millisecond. The supplied
    /// idle hook runs only immediately before a real worker wait, outside locks.
    pub fn wait_output(&mut self,timeout:Duration)->Result<Option<EncodedOutput>,MediaError> {
        let deadline=Instant::now()+timeout.min(Duration::from_millis(200));
        loop {
            if let Some(output)=self.take_output()? {return Ok(Some(output));}
            if matches!(self.flight,Flight::Idle) || !self.observation.asynchronous || !self.wait_signal(deadline)? {return Ok(None);}
        }
    }
    fn wait_signal(&mut self,deadline:Instant)->Result<bool,MediaError> {
        let remaining=deadline.saturating_duration_since(Instant::now());if remaining.is_zero() {return Ok(false);}
        (self.idle)();
        // SAFETY: Arc retains the event through this bounded kernel wait.
        let result=unsafe {WaitForSingleObject(self.signals.wake.0,remaining.as_millis().max(1)as u32)};
        if result==WAIT_TIMEOUT {return Ok(false);}
        if result==WAIT_FAILED || result!=WAIT_OBJECT_0 {self.close();return Err(MediaError::Platform {operation:"wait encoder event",code:E_FAIL.0});}
        Ok(true)
    }
    pub fn close(&mut self) {
        if matches!(self.flight,Flight::Closed) {return;}
        self.flight=Flight::Closed;self.signals.closed.store(true,Ordering::Release);self.signals.notify();
        // SAFETY: close executes on the owning MTA. Close callbacks first,
        // abort pending input/output, then break native asynchronous event refs.
        unsafe {
            let _=self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH,0);
            if let Some(shutdown)=&self.shutdown {let _=shutdown.Shutdown();}
            let _=self.activation.ShutdownObject();
        }
    }
}
impl Drop for VideoEncoder {fn drop(&mut self) {self.close();}}
struct Locked<'a> {buffer:&'a IMFMediaBuffer,data:*mut u8,capacity:usize,length:usize}
impl<'a> Locked<'a> {
    fn new(buffer:&'a IMFMediaBuffer)->Result<Self,MediaError> {
        let mut data=ptr::null_mut();let mut capacity=0;let mut length=0;
        // SAFETY: writable output locals receive the buffer-owned lock/span.
        checked("lock media buffer",unsafe {buffer.Lock(&mut data,Some(&mut capacity),Some(&mut length))})?;
        if data.is_null() {
            // SAFETY: balance the successful native lock before rejecting it.
            unsafe {let _=buffer.Unlock();}return Err(MediaError::InvalidFrame);
        }
        Ok(Self {buffer,data,capacity:capacity as usize,length:length as usize})
    }
}
impl Drop for Locked<'_> {fn drop(&mut self) {
    // SAFETY: exactly one successful Lock is balanced by this guard.
    unsafe {let _=self.buffer.Unlock();}
}}
fn subtype(codec:VideoCodec)->GUID {match codec {VideoCodec::H264=>MFVideoFormat_H264,VideoCodec::Hevc=>MFVideoFormat_HEVC,VideoCodec::Av1=>MFVideoFormat_AV1}}
fn media_type(config:EncoderConfig,color:Nv12Color,input:bool)->Result<IMFMediaType,MediaError> {
    // SAFETY: the new type is exclusively owned on this encoder MTA.
    checked("configure media type",unsafe {(||->windows_core::Result<_> {
        let media=MFCreateMediaType()?;media.SetGUID(&MF_MT_MAJOR_TYPE,&MFMediaType_Video)?;
        media.SetGUID(&MF_MT_SUBTYPE,&if input {MFVideoFormat_NV12} else {subtype(config.codec)})?;
        media.SetUINT64(&MF_MT_FRAME_SIZE,(u64::from(config.width)<<32)|u64::from(config.height))?;
        media.SetUINT64(&MF_MT_FRAME_RATE,(u64::from(config.fps)<<32)|1)?;media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO,(1u64<<32)|1)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive.0 as u32)?;color.set(&media)?;
        if !input {media.SetUINT32(&MF_MT_AVG_BITRATE,config.bitrate_kbps*1000)?;media.SetUINT32(&MF_MT_MPEG2_PROFILE,match config.codec {VideoCodec::H264=>66,VideoCodec::Hevc=>1,VideoCodec::Av1=>0})?;}
        Ok(media)
    })()})
}
fn check_info(info:&MFT_OUTPUT_STREAM_INFO)->Result<(),MediaError> {
    if info.cbSize as usize>MAX_FRAME_BYTES || info.cbAlignment>4096 || info.cbAlignment>0 && !info.cbAlignment.is_power_of_two()
        || info.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32==0 && info.cbSize==0 {return Err(MediaError::Configuration);}
    Ok(())
}
fn verify_type(media:&IMFMediaType,config:EncoderConfig,color:Nv12Color)->Result<(),MediaError> {
    let expected=media_type(config,color,false)?;
    // SAFETY: both media types are live on this MTA. A driver may add metadata,
    // but it cannot change the requested geometry, cadence, profile or colors.
    unsafe {
        for key in [&MF_MT_MAJOR_TYPE,&MF_MT_SUBTYPE] {
            if checked("accepted codec",media.GetGUID(key))?!=checked("requested codec",expected.GetGUID(key))? {return Err(MediaError::Configuration);}
        }
        for key in [&MF_MT_FRAME_SIZE,&MF_MT_FRAME_RATE,&MF_MT_PIXEL_ASPECT_RATIO] {
            if checked("accepted video geometry",media.GetUINT64(key))?!=checked("requested video geometry",expected.GetUINT64(key))? {return Err(MediaError::Configuration);}
        }
        for key in [&MF_MT_INTERLACE_MODE,&MF_MT_MPEG2_PROFILE,&MF_MT_VIDEO_PRIMARIES,&MF_MT_TRANSFER_FUNCTION,&MF_MT_YUV_MATRIX,&MF_MT_VIDEO_NOMINAL_RANGE] {
            if checked("accepted video attributes",media.GetUINT32(key))?!=checked("requested video attributes",expected.GetUINT32(key))? {return Err(MediaError::Configuration);}
        }
    }
    Ok(())
}
fn sequence_header(media:&IMFMediaType)->Result<Vec<u8>,MediaError> {
    // SAFETY: only the bounded native metadata blob is copied; its actual
    // encoded framing/profile is checked before any output is published.
    unsafe {
        let size=media.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).unwrap_or(0);
        if size>64*1024 {return Err(MediaError::InvalidFrame);}let mut data=vec![0;size as usize];
        if size>0 {checked("encoder sequence header",media.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER,&mut data,None))?;}
        Ok(data)
    }
}
