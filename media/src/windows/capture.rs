//! Thread-owned Windows Graphics Capture. Discovery does not capture/prompt;
//! opening must be authorized by the embedder's current view/sharing boundary.
//! The system cursor and capture border remain enabled. SDR only until an
//! independently qualified HDR-to-SDR path exists.
use super::{checked,Wake};
use crate::MediaError;
use std::{marker::PhantomData,ptr,rc::Rc,sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use windows_core::{Interface,IInspectable};
use ::windows::{Foundation::TypedEventHandler,Graphics::{SizeInt32,Capture::{Direct3D11CaptureFrame,Direct3D11CaptureFramePool,GraphicsCaptureItem,GraphicsCaptureSession},DirectX::{DirectXPixelFormat,Direct3D11::IDirect3DDevice}},Win32::{Foundation::{HANDLE,HMODULE,WAIT_OBJECT_0,WAIT_TIMEOUT},Graphics::{Direct3D::{D3D_DRIVER_TYPE_UNKNOWN,D3D_FEATURE_LEVEL_11_0},Direct3D11::*,Dxgi::{*,Common::*}},System::{StationsAndDesktops::*,Threading::{CreateEventW,GetCurrentThreadId,SetEvent,WaitForSingleObject},WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED,Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice,IDirect3DDxgiInterfaceAccess},Graphics::Capture::IGraphicsCaptureItemInterop}}}};

struct Apartment(PhantomData<Rc<()>>);
impl Apartment {fn new()->Result<Self,MediaError> {
    // SAFETY: balance WinRT MTA initialization on this same !Send owner thread.
    checked("capture WinRT MTA",unsafe {RoInitialize(RO_INIT_MULTITHREADED)})?;Ok(Self(PhantomData))
}}
impl Drop for Apartment {fn drop(&mut self) {
    // SAFETY: the last apartment field drops after its native interfaces.
    unsafe {RoUninitialize();}
}}
/// Locally discovered display proof. Revalidated before opening and every read;
/// an old output ordinal cannot silently select another monitor after hotplug.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct DisplayInfo {pub adapter_high:i32,pub adapter_low:u32,pub output:u32,pub name:String,pub bounds:[i32;4],pub sdr:bool}
impl DisplayInfo {
    pub fn id(&self)->String {format!("{:08x}-{:08x}-{}-{}",self.adapter_high as u32,self.adapter_low,self.output,self.name)}
    pub fn size(&self)->Result<(u32,u32),MediaError> {
        let width=i64::from(self.bounds[2])-i64::from(self.bounds[0]);let height=i64::from(self.bounds[3])-i64::from(self.bounds[1]);
        if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {return Err(MediaError::Configuration);}
        Ok((width as u32,height as u32))
    }
}
fn factory()->Result<IDXGIFactory1,MediaError> {
    // SAFETY: a new factory is returned with one owned COM reference.
    checked("capture adapter factory",unsafe {CreateDXGIFactory1()})
}
fn describe(adapter:&IDXGIAdapter1,output:u32,value:&IDXGIOutput6)->Result<DisplayInfo,MediaError> {
    // SAFETY: native descriptions are returned by value from live interfaces.
    unsafe {
        let gpu=checked("capture adapter description",adapter.GetDesc1())?;let desc=checked("capture output description",value.GetDesc1())?;
        if !desc.AttachedToDesktop.as_bool() {return Err(MediaError::Closed);}
        let end=desc.DeviceName.iter().position(|c|*c==0).ok_or(MediaError::Configuration)?;
        let name=String::from_utf16(&desc.DeviceName[..end]).map_err(|_|MediaError::Configuration)?;
        let info=DisplayInfo {adapter_high:gpu.AdapterLuid.HighPart,adapter_low:gpu.AdapterLuid.LowPart,output,name,bounds:[desc.DesktopCoordinates.left,desc.DesktopCoordinates.top,desc.DesktopCoordinates.right,desc.DesktopCoordinates.bottom],sdr:desc.ColorSpace==DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709};info.size()?;Ok(info)
    }
}
/// No capture session, screen pixels, consent prompt or application authority.
pub fn displays()->Result<Vec<DisplayInfo>,MediaError> {
    let _apartment=Apartment::new()?;let factory=factory()?;let mut values=Vec::new();
    // SAFETY: bounded native enumeration terminates on the documented sentinel.
    unsafe {
        for index in 0..16 {
            let adapter=match factory.EnumAdapters1(index) {Ok(value)=>value,Err(error)if error.code()==DXGI_ERROR_NOT_FOUND=>break,Err(error)=>return Err(super::platform("enumerate capture adapters",error))};
            for output in 0..32 {
                let native=match adapter.EnumOutputs(output) {Ok(value)=>value,Err(error)if error.code()==DXGI_ERROR_NOT_FOUND=>break,Err(error)=>return Err(super::platform("enumerate capture outputs",error))};
                let native=checked("capture output color support",native.cast::<IDXGIOutput6>())?;
                match describe(&adapter,output,&native) {Ok(info)=>values.push(info),Err(MediaError::Closed)=>{},Err(error)=>return Err(error)}
                if values.len()>64 {return Err(MediaError::Configuration);}
            }
        }
    }
    Ok(values)
}
/// One dedicated, multithread-protected hardware device. Rc keeps capture,
/// conversion and its WinRT apartment on one worker; no WARP substitution.
pub struct CaptureDevice {pub(super) device:ID3D11Device,pub(super) context:ID3D11DeviceContext,_apartment:Apartment}
impl CaptureDevice {
    fn create(adapter:&IDXGIAdapter1,apartment:Apartment)->Result<Rc<Self>,MediaError> {
        let mut device=None;let mut context=None;
        // SAFETY: outputs receive owned interfaces on this initialized MTA.
        unsafe {
            checked("create capture D3D11 device",D3D11CreateDevice(adapter,D3D_DRIVER_TYPE_UNKNOWN,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT|D3D11_CREATE_DEVICE_VIDEO_SUPPORT,Some(&[D3D_FEATURE_LEVEL_11_0]),D3D11_SDK_VERSION,Some(&mut device),None,Some(&mut context)))?;
            let device=device.ok_or(MediaError::Configuration)?;let context=context.ok_or(MediaError::Configuration)?;
            let protection=checked("capture device threading",context.cast::<ID3D11Multithread>())?;let _previous=protection.SetMultithreadProtected(true);
            Ok(Rc::new(Self {device,context,_apartment:apartment}))
        }
    }
    /// Synthetic GPU qualification only: this selects an adapter, not a screen.
    pub fn default_hardware()->Result<Rc<Self>,MediaError> {
        let apartment=Apartment::new()?;let factory=factory()?;
        // SAFETY: enumerate bounded adapters, excluding software registrations.
        unsafe {for index in 0..16 {
            let adapter=match factory.EnumAdapters1(index) {Ok(adapter)=>adapter,Err(error)if error.code()==DXGI_ERROR_NOT_FOUND=>break,Err(error)=>return Err(super::platform("fixture adapter",error))};
            if checked("fixture adapter flags",adapter.GetDesc1())?.Flags&DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32==0 {return Self::create(&adapter,apartment);}
        }}
        Err(MediaError::UnsupportedCodec)
    }
    pub fn d3d11(&self)->&ID3D11Device {&self.device}
}
struct CaptureSignal {wake:Wake,closed:AtomicBool}
impl CaptureSignal {
    fn notify(&self) {
        // SAFETY: the shared signal retains its event until the last callback.
        unsafe {let _=SetEvent(self.wake.0);}
    }
    fn close(&self) {self.closed.store(true,Ordering::Release);self.notify();}
    fn check(&self)->Result<(),MediaError> {if self.closed.load(Ordering::Acquire) {Err(MediaError::Closed)} else {Ok(())}}
}
/// Close from any thread without sending COM/capture objects across apartments.
#[derive(Clone)]
pub struct CaptureStop(Arc<CaptureSignal>);
impl CaptureStop {pub fn close(&self) {self.0.close();}}
struct Resources {pool:Option<Direct3D11CaptureFramePool>,session:Option<GraphicsCaptureSession>,item:GraphicsCaptureItem,arrived:Option<i64>,closed:Option<i64>,signal:Arc<CaptureSignal>}
impl Drop for Resources {fn drop(&mut self) {
    self.signal.close();
    if let Some(token)=self.closed.take() {let _=self.item.RemoveClosed(token);}
    if let Some(pool)=&self.pool {if let Some(token)=self.arrived.take() {let _=pool.RemoveFrameArrived(token);}}
    if let Some(session)=self.session.take() {let _=session.Close();}
    if let Some(pool)=self.pool.take() {let _=pool.Close();}
}}
pub struct DesktopCapture {resources:Resources,output:IDXGIOutput6,info:DisplayInfo,device:Rc<CaptureDevice>,idle:fn()}
impl DesktopCapture {
    /// Start only after current local sharing and authenticated view admission.
    /// Uses Win32 monitor interop; it does not open a picker or confer consent.
    pub fn open(info:&DisplayInfo,idle:fn())->Result<Self,MediaError> {
        let apartment=Apartment::new()?;let factory=factory()?;let mut found=None;
        // SAFETY: match the saved adapter LUID and revalidate the entire display
        // proof before creating any capture session. Never switch desktop/session.
        unsafe {for index in 0..16 {
            let adapter=match factory.EnumAdapters1(index) {Ok(value)=>value,Err(error)if error.code()==DXGI_ERROR_NOT_FOUND=>break,Err(error)=>return Err(super::platform("find capture adapter",error))};
            let desc=checked("capture adapter identity",adapter.GetDesc1())?;
            if desc.AdapterLuid.HighPart==info.adapter_high && desc.AdapterLuid.LowPart==info.adapter_low {found=Some(adapter);break;}
        }}
        let adapter=found.ok_or(MediaError::Closed)?;
        // SAFETY: the caller's plain metadata is checked against current output.
        let output=checked("find capture output",unsafe {adapter.EnumOutputs(info.output)})?;
        let output=checked("capture output interface",output.cast::<IDXGIOutput6>())?;
        if describe(&adapter,info.output,&output)?!=*info || !info.sdr || !active_desktop()? {return Err(MediaError::Closed);}
        let device=CaptureDevice::create(&adapter,apartment)?;
        if !checked("Windows capture support",GraphicsCaptureSession::IsSupported())? {return Err(MediaError::UnsupportedCodec);}
        let interop:IGraphicsCaptureItemInterop=checked("monitor capture interop",windows_core::factory::<GraphicsCaptureItem,IGraphicsCaptureItemInterop>())?;
        // SAFETY: the monitor handle was obtained from this revalidated output.
        let item:GraphicsCaptureItem=unsafe {checked("capture item",interop.CreateForMonitor(checked("capture monitor",output.GetDesc1())?.Monitor))?};
        let (width,height)=info.size()?;let size=checked("capture item size",item.Size())?;
        if size!=(SizeInt32 {Width:width as i32,Height:height as i32}) {return Err(MediaError::Closed);}
        // SAFETY: create a kernel event and project our protected DXGI device.
        let wake=checked("capture wake event",unsafe {CreateEventW(None,false,false,None)})?;
        let signal=Arc::new(CaptureSignal {wake:Wake(wake),closed:AtomicBool::new(false)});
        let mut resources=Resources {pool:None,session:None,item,arrived:None,closed:None,signal:Arc::clone(&signal)};
        let dxgi=checked("capture DXGI device",device.device.cast::<IDXGIDevice>())?;
        let projected:IDirect3DDevice=checked("capture WinRT device",unsafe {CreateDirect3D11DeviceFromDXGIDevice(&dxgi)})?.cast().map_err(|e|super::platform("project capture device",e))?;
        let pool=checked("capture frame pool",Direct3D11CaptureFramePool::CreateFreeThreaded(&projected,DirectXPixelFormat::B8G8R8A8UIntNormalized,2,size))?;
        resources.pool=Some(pool.clone());
        let session=checked("capture session",pool.CreateCaptureSession(&resources.item))?;resources.session=Some(session.clone());
        checked("capture system cursor",session.SetIsCursorCaptureEnabled(true))?;
        if !checked("capture border required",session.IsBorderRequired())? {return Err(MediaError::Configuration);}
        let arrived=Arc::clone(&signal);resources.arrived=Some(checked("capture frame event",pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool,IInspectable>::new(move|_,_|{arrived.notify();Ok(())})))?);
        resources.closed=Some(checked("capture close event",resources.item.Closed(&TypedEventHandler::<GraphicsCaptureItem,IInspectable>::new(move|_,_|{signal.close();Ok(())})))?);
        checked("start screen capture",session.StartCapture())?;
        Ok(Self {resources,output,info:info.clone(),device,idle})
    }
    pub fn device(&self)->Rc<CaptureDevice> {Rc::clone(&self.device)}
    pub fn stop_handle(&self)->CaptureStop {CaptureStop(Arc::clone(&self.resources.signal))}
    pub fn close(&self) {
        self.resources.signal.close();
        if let Some(session)=&self.resources.session {let _=session.Close();}
        if let Some(pool)=&self.resources.pool {let _=pool.Close();}
    }
    pub fn current(&self)->Result<(),MediaError> {
        if let Err(error)=self.resources.signal.check() {self.close();return Err(error);}
        // SAFETY: current live output metadata only; never capture another output.
        let desc=checked("current capture output",unsafe {self.output.GetDesc1()})?;
        if !active_desktop()? || !desc.AttachedToDesktop.as_bool() || desc.ColorSpace!=DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
            || [desc.DesktopCoordinates.left,desc.DesktopCoordinates.top,desc.DesktopCoordinates.right,desc.DesktopCoordinates.bottom]!=self.info.bounds {self.close();return Err(MediaError::Closed);}
        Ok(())
    }
    /// Coalesce at most the two frame-pool entries. Returned native frames keep
    /// the pool/device alive and are explicitly checked back in on Drop.
    pub fn next_frame(&self,timeout:Duration)->Result<Option<CapturedFrame<'_>>,MediaError> {
        let deadline=Instant::now()+timeout.min(Duration::from_millis(200));
        loop {
            self.current()?;let mut latest=None;
            for _ in 0..2 {
                let Some(frame)=self.pull()? else {break;};latest=Some(frame);
            }
            if let Some(frame)=latest {return Ok(Some(frame));}
            let remaining=deadline.saturating_duration_since(Instant::now());if remaining.is_zero() {return Ok(None);}
            // SAFETY: callback/state own the same live auto-reset event. Check
            // ready work first; the idle hook is only before a bounded real wait.
            let ready=unsafe {WaitForSingleObject(self.resources.signal.wake.0,0)};
            if ready==WAIT_OBJECT_0 {continue;}if ready!=WAIT_TIMEOUT {self.close();return Err(MediaError::Closed);}
            (self.idle)();
            let result=unsafe {WaitForSingleObject(self.resources.signal.wake.0,remaining.as_millis().max(1)as u32)};
            if result==WAIT_TIMEOUT {self.current()?;return Ok(None);}if result!=WAIT_OBJECT_0 {self.close();return Err(MediaError::Closed);}
        }
    }
    fn pull(&self)->Result<Option<CapturedFrame<'_>>,MediaError> {
        let pool=self.resources.pool.as_ref().ok_or(MediaError::Closed)?;let mut raw=ptr::null_mut();
        // SAFETY: this API returns S_OK/null when empty. Preserve that distinction
        // rather than interpreting the generated nonnullable projection's error.
        checked("next capture frame",unsafe {(Interface::vtable(pool).TryGetNextFrame)(pool.as_raw(),&mut raw).ok()})?;
        if raw.is_null() {return Ok(None);}
        // SAFETY: one successful nonnull WinRT frame reference transfers to us.
        let frame=unsafe {Direct3D11CaptureFrame::from_raw(raw)};
        let owned=Frame(frame);let size=checked("captured content size",owned.0.ContentSize())?;let (width,height)=self.info.size()?;
        if size.Width!=width as i32 || size.Height!=height as i32 {self.close();return Err(MediaError::Closed);}
        let timestamp=checked("capture QPC time",owned.0.SystemRelativeTime())?.Duration;
        if timestamp<0 {return Err(MediaError::InvalidFrame);}
        let surface=checked("captured surface",owned.0.Surface())?;
        let access=checked("captured DXGI surface",surface.cast::<IDirect3DDxgiInterfaceAccess>())?;
        // SAFETY: the frame retains this resource; its wrapper cannot be closed
        // until conversion's GPU fence has completed all reads of these pixels.
        let texture=checked("captured texture",unsafe {access.GetInterface::<ID3D11Texture2D>()})?;
        Ok(Some(CapturedFrame {frame:owned,texture,pts:timestamp/10,capture:self}))
    }
}
impl Drop for DesktopCapture {fn drop(&mut self) {self.close();}}
struct Frame(Direct3D11CaptureFrame);
impl Drop for Frame {fn drop(&mut self) {let _=self.0.Close();}}
pub struct CapturedFrame<'a> {frame:Frame,pub(super) texture:ID3D11Texture2D,pts:i64,capture:&'a DesktopCapture}
impl CapturedFrame<'_> {
    pub fn presentation_time_micros(&self)->i64 {self.pts}
    pub(super) fn validate(&self)->Result<(),MediaError> {let _=&self.frame;self.capture.current()}
    pub(super) fn capture_close(&self) {self.capture.close();}
}
#[derive(PartialEq,Eq)]
struct DesktopName {value:[u16;128],length:usize}
fn desktop_name(handle:HANDLE)->Result<DesktopName,MediaError> {
    let mut name=[0u16;128];let mut bytes=0;
    // SAFETY: bounded writable UTF-16 output, exact byte capacity is declared.
    checked("input desktop name",unsafe {GetUserObjectInformationW(handle,UOI_NAME,Some(name.as_mut_ptr().cast()),(name.len()*2)as u32,Some(&mut bytes))})?;
    let count=usize::try_from(bytes).map_err(|_|MediaError::Configuration)?;
    if count<2 || count>name.len()*2 || !count.is_multiple_of(2) || name[count/2-1]!=0 {return Err(MediaError::Configuration);}
    Ok(DesktopName {value:name,length:count/2-1})
}
fn active_desktop()->Result<bool,MediaError> {
    // SAFETY: inspect the input desktop with read-only access, and balance only
    // our OpenInputDesktop handle. Never switch/elevate or close ThreadDesktop.
    unsafe {
        let input=match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0),false,DESKTOP_READOBJECTS) {Ok(input)=>input,Err(_)=>return Ok(false)};
        let input_name=desktop_name(HANDLE(input.0));let _=CloseDesktop(input);
        let own=checked("worker desktop",GetThreadDesktop(GetCurrentThreadId()))?;
        Ok(input_name?==desktop_name(HANDLE(own.0))?)
    }
}
