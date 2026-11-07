//! Bound relay allocation fanout without changing host/STUN candidates. UDP
//! follows the OS route when that IP is among the available sockets; explicit
//! bindings without that route keep their existing fallback. TCP/TLS adapters
//! need one local IPC socket per address family, independent of egress routing.

use std::{collections::HashSet,fmt,future::Future,io,net::{IpAddr,SocketAddr,UdpSocket},pin::Pin,sync::{Arc,Mutex},time::{Duration,Instant}};
use webrtc::runtime::{AsyncInterval,AsyncTcpListener,AsyncTcpStream,AsyncUdpSocket,JoinHandle,Runtime,TokioRuntime};

#[derive(Clone)]
struct RelayConfiguration {ipc:HashSet<SocketAddr>,turn_delay:Duration}
pub(super) struct RelayRuntime {inner:TokioRuntime,configuration:Mutex<RelayConfiguration>}
impl Default for RelayRuntime {fn default()->Self {Self::new(HashSet::new())}}
impl fmt::Debug for RelayRuntime {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.debug_struct("RelayRuntime").field("ipc_endpoints",&self.configuration.lock().unwrap_or_else(|error|error.into_inner()).ipc.len()).finish()}
}
impl RelayRuntime {
    pub fn new(ipc:HashSet<SocketAddr>)->Self {Self {inner:TokioRuntime,configuration:Mutex::new(RelayConfiguration {ipc,turn_delay:Duration::ZERO})}}
    pub fn with_turn_delay(self,delay:Duration)->Self {self.configuration.lock().unwrap_or_else(|error|error.into_inner()).turn_delay=delay;self}
    pub fn include_ipc(&self,other:&Self) {
        let added=other.configuration.lock().unwrap_or_else(|error|error.into_inner()).ipc.clone();
        self.configuration.lock().unwrap_or_else(|error|error.into_inner()).ipc.extend(added);
    }
    pub fn replace_configuration(&self,other:&Self) {
        let current=other.configuration.lock().unwrap_or_else(|error|error.into_inner()).clone();
        *self.configuration.lock().unwrap_or_else(|error|error.into_inner())=current;
    }
    fn preferred(&self,server:SocketAddr,available:&[SocketAddr])->Option<SocketAddr> {
        let family=|address:&&SocketAddr|address.is_ipv4()==server.is_ipv4();
        let selected=if self.configuration.lock().unwrap_or_else(|error|error.into_inner()).ipc.contains(&server) {
            // IPC remains local. The adapter's TCP connection independently
            // follows the OS route to the original TURN endpoint.
            available.iter().filter(family).min()
        } else {
            let route=route_ip(server)?;
            // No blind first-interface choice for real Internet UDP. The
            // caller may explicitly bind another valid route or a VPN.
            available.iter().filter(family).filter(|address|address.ip()==route).min()
        };
        selected.copied()
    }
}
fn route_ip(server:SocketAddr)->Option<IpAddr> {
    // UDP connect selects a local route without sending a datagram, waiting
    // for a handshake, resolving DNS or starting a worker thread. Recompute
    // from the actual destination/bindings on every gather after a handover.
    let socket=UdpSocket::bind(if server.is_ipv4() {"0.0.0.0:0"} else {"[::]:0"}).ok()?;
    socket.connect(server).ok()?;Some(socket.local_addr().ok()?.ip())
}
impl Runtime for RelayRuntime {
    fn turn_gathering_delay(&self)->Duration {self.configuration.lock().unwrap_or_else(|error|error.into_inner()).turn_delay}
    fn preferred_turn_local_address(&self,server:SocketAddr,available:&[SocketAddr])->Option<SocketAddr> {self.preferred(server,available)}
    fn spawn(&self,future:Pin<Box<dyn Future<Output=()>+Send>>)->Box<dyn JoinHandle> {self.inner.spawn(future)}
    fn spawn_reactor(&self,size:usize,future:Pin<Box<dyn Future<Output=()>+Send>>)->Box<dyn JoinHandle> {self.inner.spawn_reactor(size,future)}
    fn wrap_udp_socket(&self,socket:UdpSocket)->io::Result<Arc<dyn AsyncUdpSocket>> {self.inner.wrap_udp_socket(socket)}
    fn wrap_tcp_listener(&self,listener:std::net::TcpListener)->io::Result<Arc<dyn AsyncTcpListener>> {self.inner.wrap_tcp_listener(listener)}
    fn connect_tcp<'a>(&'a self,remote:SocketAddr)->Pin<Box<dyn Future<Output=io::Result<Arc<dyn AsyncTcpStream>>>+Send+'a>> {self.inner.connect_tcp(remote)}
    fn resolve_host<'a>(&'a self,host:&'a str)->Pin<Box<dyn Future<Output=io::Result<Vec<SocketAddr>>>+Send+'a>> {self.inner.resolve_host(host)}
    fn now(&self)->Instant {self.inner.now()}
    fn sleep(&self,duration:Duration)->Pin<Box<dyn Future<Output=()>+Send+'static>> {self.inner.sleep(duration)}
    fn interval(&self,period:Duration)->Box<dyn AsyncInterval> {self.inner.interval(period)}
    fn block_on(&self,future:Pin<Box<dyn Future<Output=()>+'_>>) {self.inner.block_on(future);}
    fn yield_now(&self)->Pin<Box<dyn Future<Output=()>+Send+'static>> {self.inner.yield_now()}
    fn name(&self)->&'static str {"tokio-relay-interfaces"}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_replaces_ipc_addresses_and_turn_grace_instead_of_retaining_old_generations() {
        let old:SocketAddr="127.0.0.1:41000".parse().unwrap();let current:SocketAddr="127.0.0.1:42000".parse().unwrap();
        let runtime=RelayRuntime::new([old].into_iter().collect());
        runtime.replace_configuration(&RelayRuntime::new([current].into_iter().collect()).with_turn_delay(Duration::from_millis(250)));
        assert_eq!(runtime.configuration.lock().unwrap().ipc,[current].into_iter().collect::<HashSet<_>>());
        assert_eq!(runtime.turn_gathering_delay(),Duration::from_millis(250));
        runtime.replace_configuration(&RelayRuntime::default());assert!(runtime.configuration.lock().unwrap().ipc.is_empty());
        assert_eq!(runtime.turn_gathering_delay(),Duration::ZERO);
    }
    #[test]
    fn relay_interfaces_follow_udp_route_and_keep_explicit_alternatives() {
        let runtime=RelayRuntime::default();let server="127.0.0.1:3478".parse().unwrap();
        let available=["127.0.0.1:40001".parse().unwrap(),"127.0.0.2:40000".parse().unwrap(),"127.0.0.1:40000".parse().unwrap()];
        assert_eq!(runtime.preferred(server,&available),Some(available[2]));
        // A named alternate binding must remain usable when the default
        // route is not one of the caller's sockets, including after a rebind.
        let rebound=[available[1]];assert_eq!(runtime.preferred(server,&rebound),None);
    }
    #[test]
    fn relay_interfaces_limit_ipc_per_family_and_follow_rebound_sockets() {
        let ipv4="127.0.0.1:3478".parse().unwrap();let ipv6="[::1]:3478".parse().unwrap();
        let runtime=RelayRuntime::new([ipv4,ipv6].into_iter().collect());
        let available=["127.0.0.1:40000".parse().unwrap(),"127.0.0.2:40000".parse().unwrap(),"[::1]:40000".parse().unwrap(),"[::2]:40000".parse().unwrap()];
        for (server,expected) in [(ipv4,0),(ipv6,2)] {
            assert_eq!(runtime.preferred(server,&available),Some(available[expected]));
        }
        let rebound=[available[1],available[3]];
        assert_eq!(runtime.preferred(ipv4,&rebound),Some(rebound[0]));assert_eq!(runtime.preferred(ipv6,&rebound),Some(rebound[1]));
        assert!(!format!("{runtime:?}").contains("127.0.0.1"));
    }
}
