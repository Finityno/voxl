//! Adapt the pinned SDK's TURN datagrams to RFC 8656 TCP/TLS connections.
//! This is endpoint-local IPC, never a public proxy. Each SDK socket owns its
//! own stream allocation. Direct ICE and ordinary TURN/UDP bypass this module.
use super::WebRtcError;
use rtc::ice::url::{ProtoType,SchemeType};
use std::{collections::HashMap,io,net::{IpAddr,SocketAddr},sync::{Arc,Mutex}};
use tokio::{io::{AsyncRead,AsyncReadExt,AsyncWrite,AsyncWriteExt},net::{TcpStream,UdpSocket},sync::mpsc,task::{AbortHandle,JoinSet},time::{Duration,timeout}};
use tokio_rustls::{TlsConnector,rustls::{self,ClientConfig,pki_types::ServerName}};
use webrtc::peer_connection::RTCIceServer;

const MAX_DATAGRAM: usize = 65_507;
const QUEUED_DATAGRAMS: usize = 32;

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum RelayTransport { #[default] Unknown, Udp, Tcp, Tls }

/// Dropping an opening peer or closing a live view cancels every stream,
/// including DNS, handshake and blocked writes. No detached retry survives it.
#[derive(Default)]
pub(super) struct StreamRelays {
    listeners: Vec<AbortHandle>,
    allocations: Arc<Mutex<HashMap<SocketAddr,RelayTransport>>>,
}
impl StreamRelays {
    pub async fn prepare(servers: &mut Vec<RTCIceServer>, bindings: &[String], tls: Option<Arc<ClientConfig>>) -> Result<Self,WebRtcError> {
        let mut relays=Self::default();
        let allowed: Vec<IpAddr> = bindings.iter().map(|binding|binding.parse::<SocketAddr>().map(|value|value.ip()).map_err(|_|WebRtcError::Configuration)).collect::<Result<_,_>>()?;
        let mut rewritten=Vec::new();
        for server in servers.iter() {
            for url in server.urls().map_err(|_|WebRtcError::Configuration)? {
                if !matches!(url.scheme,SchemeType::Turn|SchemeType::Turns) || url.proto!=ProtoType::Tcp {
                    // DTLS TURN is not implemented by this adapter or the SDK.
                    if url.is_secure() {return Err(WebRtcError::Configuration);}
                    rewritten.push(RTCIceServer {urls:vec![url.to_string()],username:server.username.clone(),credential:server.credential.clone()});
                    continue;
                }
                if allowed.iter().any(IpAddr::is_unspecified) {return Err(WebRtcError::Configuration);}
                let transport=if url.is_secure() {RelayTransport::Tls} else {RelayTransport::Tcp};
                let target=Arc::new(Target {host:url.host,port:url.port,transport,tls:if transport==RelayTransport::Tls {Some(match &tls {Some(value)=>Arc::clone(value),None=>default_tls()?})} else {None}});
                let ipv4=allowed.iter().any(IpAddr::is_ipv4);
                let ipv6=allowed.iter().any(IpAddr::is_ipv6);
                let first=UdpSocket::bind(if ipv4 {"127.0.0.1:0"} else {"[::1]:0"}).await.map_err(|_|WebRtcError::Transport)?;
                let port=first.local_addr().map_err(|_|WebRtcError::Transport)?.port();
                let mut sockets=vec![first];
                if ipv4 && ipv6 {sockets.push(UdpSocket::bind(SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),port)).await.map_err(|_|WebRtcError::Transport)?);}
                for socket in sockets {
                    let task=tokio::spawn(listen(Arc::new(socket),Arc::clone(&target),allowed.clone(),Arc::clone(&relays.allocations)));
                    relays.listeners.push(task.abort_handle());
                }
                rewritten.push(RTCIceServer {urls:vec![format!("turn:localhost:{port}?transport=udp")],username:server.username.clone(),credential:server.credential.clone()});
            }
        }
        *servers=rewritten;
        Ok(relays)
    }
    pub fn close(&self) {for task in &self.listeners {task.abort();}}
    pub fn transport(&self,address: SocketAddr) -> RelayTransport {
        self.allocations.lock().unwrap_or_else(|error|error.into_inner()).get(&address).copied().unwrap_or_default()
    }
}
impl Drop for StreamRelays {fn drop(&mut self) {self.close();}}

fn default_tls() -> Result<Arc<ClientConfig>,WebRtcError> {
    let roots=rustls::RootCertStore {roots:webpki_roots::TLS_SERVER_ROOTS.to_vec()};
    let config=ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions().map_err(|_|WebRtcError::Configuration)?.with_root_certificates(roots).with_no_client_auth();
    Ok(Arc::new(config))
}
struct Target {host:String,port:u16,transport:RelayTransport,tls:Option<Arc<ClientConfig>>}

async fn listen(socket:Arc<UdpSocket>,target:Arc<Target>,allowed:Vec<IpAddr>,allocations:Arc<Mutex<HashMap<SocketAddr,RelayTransport>>>) {
    let mut sessions: HashMap<SocketAddr,Option<mpsc::Sender<Vec<u8>>>>=HashMap::new();
    let mut workers=JoinSet::new();
    let mut buffer=vec![0;MAX_DATAGRAM+1];
    loop {
        tokio::select! {
            result=socket.recv_from(&mut buffer) => {
                let Ok((length,source))=result else {break;};
                if !allowed.contains(&source.ip()) || length>MAX_DATAGRAM || datagram_length(&buffer[..length]).is_err() {continue;}
                if let Some(slot)=sessions.get_mut(&source) {
                    // Overflow ends this stream. Reopening an allocation behind
                    // an already advertised relay address would be incorrect.
                    if slot.as_ref().is_some_and(|sender|sender.try_send(buffer[..length].to_vec()).is_err()) {*slot=None;}
                    continue;
                }
                // A single allocation per local source address prevents local
                // port churn from creating unbounded streams. Failed addresses
                // remain reserved until this view ends; ICE restart owns retry.
                if sessions.len()>=allowed.len() || sessions.keys().any(|other|other.ip()==source.ip()) {continue;}
                let (sender,receiver)=mpsc::channel(QUEUED_DATAGRAMS);
                if sender.try_send(buffer[..length].to_vec()).is_err() {continue;}
                sessions.insert(source,Some(sender));
                workers.spawn(session(Arc::clone(&socket),source,Arc::clone(&target),receiver,Arc::clone(&allocations)));
            },
            _=workers.join_next(),if !workers.is_empty()=>{},
        }
    }
}

async fn session(socket:Arc<UdpSocket>,source:SocketAddr,target:Arc<Target>,receiver:mpsc::Receiver<Vec<u8>>,allocations:Arc<Mutex<HashMap<SocketAddr,RelayTransport>>>) -> io::Result<()> {
    let stream=timeout(Duration::from_secs(2),TcpStream::connect((target.host.as_str(),target.port))).await.map_err(|_|deadline())??;
    stream.set_nodelay(true)?;
    if let Some(config)=&target.tls {
        let name=ServerName::try_from(target.host.clone()).map_err(|_|io::Error::from(io::ErrorKind::InvalidInput))?;
        let stream=timeout(Duration::from_secs(2),TlsConnector::from(Arc::clone(config)).connect(name,stream)).await.map_err(|_|deadline())??;
        forward(stream,socket,source,receiver,target.transport,allocations).await
    } else {forward(stream,socket,source,receiver,target.transport,allocations).await}
}

async fn forward<S:AsyncRead+AsyncWrite+Unpin>(stream:S,socket:Arc<UdpSocket>,source:SocketAddr,mut receiver:mpsc::Receiver<Vec<u8>>,transport:RelayTransport,allocations:Arc<Mutex<HashMap<SocketAddr,RelayTransport>>>) -> io::Result<()> {
    let (mut read,mut write)=tokio::io::split(stream);
    let outbound=async {
        while let Some(mut data)=receiver.recv().await {
            let length=datagram_length(&data)?;
            data.resize(length.next_multiple_of(4),0);
            timeout(Duration::from_millis(200),async {write.write_all(&data).await?;write.flush().await}).await.map_err(|_|deadline())??;
        }
        Err::<(),_>(io::Error::from(io::ErrorKind::BrokenPipe))
    };
    let inbound=async {
        let mut recorded=false;
        loop {
            // One reader future owns the frame across awaits. Cancelling a
            // read_exact on every outbound packet would lose partial headers.
            let data=timeout(Duration::from_secs(120),read_frame(&mut read)).await.map_err(|_|deadline())??;
            if !recorded {recorded=record_allocation(&data,transport,&allocations);}
            timeout(Duration::from_millis(200),socket.send_to(&data,source)).await.map_err(|_|deadline())??;
        }
        #[allow(unreachable_code)]
        Ok::<(),io::Error>(())
    };
    tokio::try_join!(outbound,inbound)?;
    Ok(())
}

fn deadline() -> io::Error {io::Error::from(io::ErrorKind::TimedOut)}

/// STUN has its own 20-byte header. ChannelData has a four-byte header and
/// mandatory stream padding; no RFC 4571 length prefix is added to either.
fn datagram_length(data:&[u8]) -> io::Result<usize> {
    if data.len()<4 {return Err(io::ErrorKind::InvalidData.into());}
    let payload=usize::from(u16::from_be_bytes([data[2],data[3]]));
    let length=match data[0]>>6 {
        0 if data.len()>=20 && data[4..8]==[0x21,0x12,0xa4,0x42] && payload.is_multiple_of(4)=>20+payload,
        1=>4+payload,
        _=>return Err(io::ErrorKind::InvalidData.into()),
    };
    if length>MAX_DATAGRAM || data.len()<length || data.len()>length.next_multiple_of(4) {return Err(io::ErrorKind::InvalidData.into());}
    Ok(length)
}

async fn read_frame<R:AsyncRead+Unpin>(read:&mut R) -> io::Result<Vec<u8>> {
    let mut header=[0;4];read.read_exact(&mut header).await?;
    let payload=usize::from(u16::from_be_bytes([header[2],header[3]]));
    let length=match header[0]>>6 {0 if payload.is_multiple_of(4)=>20+payload,1=>4+payload,_=>return Err(io::ErrorKind::InvalidData.into())};
    if length>MAX_DATAGRAM {return Err(io::ErrorKind::InvalidData.into());}
    let mut data=vec![0;length.next_multiple_of(4)];data[..4].copy_from_slice(&header);
    read.read_exact(&mut data[4..]).await?;
    datagram_length(&data)?;
    data.truncate(length);
    Ok(data)
}

fn record_allocation(data:&[u8],transport:RelayTransport,allocations:&Mutex<HashMap<SocketAddr,RelayTransport>>) -> bool {
    use rtc::stun::message::{Getter,Message,CLASS_SUCCESS_RESPONSE,METHOD_ALLOCATE};
    use rtc::turn::proto::relayaddr::RelayedAddress;
    if data.len()<20 || data[0]>>6!=0 {return false;}
    let mut message=Message::new();message.raw=data.to_vec();
    if message.decode().is_err() || message.typ.class!=CLASS_SUCCESS_RESPONSE || message.typ.method!=METHOD_ALLOCATE {return false;}
    let mut address=RelayedAddress::default();
    if address.get_from(&message).is_ok() {
        allocations.lock().unwrap_or_else(|error|error.into_inner()).insert(SocketAddr::new(address.ip,address.port),transport);
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
