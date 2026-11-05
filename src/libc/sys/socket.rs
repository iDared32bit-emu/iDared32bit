/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `sys/socket.h` (Sockets)
//!
//! We currently support blocking TCP and UDP guest sockets on IPv4 addresses.
//!
//! Because fine grain control is needed, those are implemented as
//! _non-blocking_ host sockets. Moreover, app usage of select() is
//! (optimistically) assumed to check for data readiness before calling
//! any of blocking functions.
//! (Check related functions for more details and remediation.)
//!
//! When network access is off (the default, see `--allow-network-access`),
//! sockets behave like on a device in airplane mode: apps can create, set up
//! and bind them, but they're bound to this machine only, and connecting or
//! sending to anywhere else fails straight away with [ENETUNREACH]. Apps
//! commonly set up networking when they start, and handle being offline.
//!
//! Other note: Rust std::net APIs are "too high level" sometimes,
//! thus some workarounds need to be implemented.
//! (e.g. [TcpListener] does both bind() and listen() on a call
//! to [TcpListener::bind])
//!
//! Useful resources:
//! - [Beej's Guide to Network Programming](https://beej.us/guide/bgnet/html/index-wide.html)

use crate::dyld::{export_c_func, FunctionExports};
use crate::libc::errno::{
    set_errno, EAGAIN, EBADF, ECONNREFUSED, ECONNRESET, EHOSTUNREACH, EINVAL, ENETUNREACH,
    ENOTCONN, ETIMEDOUT,
};
use crate::libc::posix_io::{
    close, find_or_create_socket, is_nonblocking, is_socket, pipe_is_ready, FileDescriptor,
};
use crate::libc::time::timeval;
use crate::mem::{
    guest_size_of, ConstPtr, ConstVoidPtr, GuestUSize, MutPtr, MutVoidPtr, Ptr, SafeRead,
};
use crate::Environment;

use crate::abi::DotDotDot;
use crate::libc::netdb::{socklen_t, IPPROTO_TCP, IPPROTO_UDP};
use std::collections::{HashMap, HashSet};
use std::io;
use std::io::{Read, Write};
use std::net::{SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

pub const AF_INET: i32 = 2;
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;

const SOL_SOCKET: i32 = 0xffff;
const SO_DEBUG: i32 = 0x1;
const SO_REUSEADDR: i32 = 0x4;
const SO_BROADCAST: i32 = 0x20;
const SO_KEEPALIVE: i32 = 0x8;
const SO_LINGER: i32 = 0x80;
const SO_SNDBUF: i32 = 0x1001;
const SO_RCVBUF: i32 = 0x1002;
const SO_SNDTIMEO: i32 = 0x1005;
const SO_RCVTIMEO: i32 = 0x1006;
const SO_NOSIGPIPE: i32 = 0x1022;
const TCP_NODELAY: i32 = 0x1;
const SO_ERROR: i32 = 0x1007;

#[allow(non_camel_case_types)]
pub type sa_family_t = u8;

const FD_SETSIZE: i32 = 1024;

/// Whether network access is off and `address` isn't on this machine, so
/// reaching it should fail as it would in airplane mode.
fn is_offline_destination(env: &Environment, address: &SocketAddrV4) -> bool {
    !env.options.network_access && !address.ip().is_loopback()
}

/// recv() flags.
const MSG_TRUNC: i32 = 0x10;
const MSG_DONTWAIT: i32 = 0x80;

/// How long connect() waits for a server to answer. touchHLE runs on one host
/// thread, so while connect() waits, everything freezes. A real device gives
/// up after about a minute and a quarter, but an app that can't reach its
/// server, perhaps because it no longer exists, should find out much sooner.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct sockaddr {
    sa_len: u8,
    sa_family: sa_family_t,
    sa_data: [u8; 14],
}
unsafe impl SafeRead for sockaddr {}
impl sockaddr {
    /// Makes an IPv4 sockaddr from 4 bytes for ip and a port.
    ///
    /// Port is expected to be native endian and
    /// will be converted to big endian internally.
    pub fn from_ipv4_parts(octets: [u8; 4], port: u16) -> Self {
        let mut addr = sockaddr {
            sa_len: 16,
            sa_family: AF_INET as u8,
            sa_data: [0; 14],
        };
        addr.sa_data[0..2].copy_from_slice(&port.to_be_bytes());
        addr.sa_data[2..6].copy_from_slice(&octets);
        addr
    }
    /// Returns 4 bytes for ip and a port.
    ///
    /// Port is returned in the native endian format.
    fn to_ipv4_parts(self) -> ([u8; 4], u16) {
        assert!(self.sa_len == 16 || self.sa_len == 0);
        assert_eq!(self.sa_family, AF_INET as u8);
        let port = u16::from_be_bytes([self.sa_data[0], self.sa_data[1]]);
        let ip = [
            self.sa_data[2],
            self.sa_data[3],
            self.sa_data[4],
            self.sa_data[5],
        ];
        (ip, port)
    }
    fn from_sockaddr_v4(addr: &SocketAddr) -> Self {
        // Only IPV4 for the moment
        assert!(addr.is_ipv4());
        let SocketAddr::V4(ipv4addr) = addr else {
            unreachable!()
        };
        sockaddr::from_ipv4_parts(ipv4addr.ip().octets(), ipv4addr.port())
    }
    pub fn to_sockaddr_v4(self) -> SocketAddrV4 {
        let (ip, port) = self.to_ipv4_parts();
        SocketAddrV4::new(ip.into(), port)
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct fd_set {
    // 32 4-byte ints should be enough for 1024 file descriptors
    fds_bits: [i32; 32],
}
unsafe impl SafeRead for fd_set {}

struct SocketHostObject {
    /// Type of the socket, [SOCK_STREAM] for TCP or [SOCK_DGRAM] for UDP
    type_: i32,
    /// Set of options
    options: HashSet<i32>,
    /// TCP socket which is yet to be connected
    tcp_listener: Option<TcpListener>,
    /// TCP socket which was connected on host, but not (yet) on the guest side
    pending_tcp_stream: Option<TcpStream>,
    /// Already connected TCP socket
    tcp_stream: Option<TcpStream>,
    /// UDP socket
    udp_socket: Option<UdpSocket>,
}

#[derive(Default)]
pub struct State {
    sockets: HashMap<i32, SocketHostObject>,
}
impl State {
    fn get(env: &Environment) -> &Self {
        &env.libc_state.socket
    }
    fn get_mut(env: &mut Environment) -> &mut Self {
        &mut env.libc_state.socket
    }
}

fn socket(env: &mut Environment, domain: i32, type_: i32, protocol: i32) -> FileDescriptor {
    // TODO: handle errno properly
    set_errno(env, 0);

    if !env.options.network_access {
        log_once!("Network access is off, so sockets work as in airplane mode: connecting or sending to other machines will fail. Use --allow-network-access to change this.");
    }

    assert_eq!(domain, AF_INET);
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);
    assert!(protocol == IPPROTO_TCP || protocol == IPPROTO_UDP || protocol == 0);

    let fd = find_or_create_socket(env);
    assert!(!State::get(env).sockets.contains_key(&fd));
    let host_object = SocketHostObject {
        type_,
        options: Default::default(),
        tcp_listener: None,
        pending_tcp_stream: None,
        tcp_stream: None,
        udp_socket: None,
    };
    State::get_mut(env).sockets.insert(fd, host_object);

    log_dbg!("socket({}, {}, {}) => {}", domain, type_, protocol, fd);
    fd
}

fn ioctl(env: &mut Environment, fd: i32, request: u32, _args: DotDotDot) -> i32 {
    assert!(is_socket(env, fd));
    log!("TODO: ioctl({} (socket), {:#x?}, ...) => -1", fd, request);
    -1
}

fn getsockopt(
    env: &mut Environment,
    socket: i32,
    level: i32,
    option_name: i32,
    option_value: MutVoidPtr,
    option_len: MutPtr<socklen_t>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "getsockopt({}, {:#x}, {:#x}, {:?}, {:?})",
        socket,
        level,
        option_name,
        option_value,
        option_len
    );

    assert_eq!(level, SOL_SOCKET);
    // TODO: support other options
    assert_eq!(option_name, SO_ERROR);

    let option_len_val = env.mem.read(option_len);
    assert_eq!(option_len_val, 4);

    let option_value: MutPtr<i32> = option_value.cast();
    env.mem.write(option_value, 0); // no errors

    0 // Success
}

fn setsockopt(
    env: &mut Environment,
    socket: i32,
    level: i32,
    option_name: i32,
    option_value: ConstVoidPtr,
    option_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "setsockopt({}, {:#x}, {:#x}, {:?}, {})",
        socket,
        level,
        option_name,
        option_value,
        option_len
    );

    if level == SOL_SOCKET && option_name == SO_DEBUG {
        set_errno(env, EINVAL);
        log!(
            "Warning: Ignore setsockopt SO_DEBUG at level {} for socket {} => -1",
            level,
            socket
        );
        return -1;
    }

    let Some(sock) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = sock.type_;

    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    match (level, option_name) {
        // TODO: SO_REUSEADDR is not supported in std::net (and not so portable)
        (SOL_SOCKET, SO_REUSEADDR | SO_BROADCAST) => {
            assert_eq!(option_len, guest_size_of::<i32>());
            let tmp: ConstPtr<i32> = option_value.cast();
            let enabled = env.mem.read(tmp) != 0;
            let options = &mut State::get_mut(env)
                .sockets
                .get_mut(&socket)
                .unwrap()
                .options;
            if enabled {
                options.insert(option_name);
            } else {
                options.remove(&option_name);
            }
        }
        // Options that only tune how the connection behaves. The host's
        // sockets work without them, so they're accepted and ignored.
        (
            SOL_SOCKET,
            SO_KEEPALIVE | SO_LINGER | SO_SNDBUF | SO_RCVBUF | SO_SNDTIMEO | SO_RCVTIMEO
            | SO_NOSIGPIPE,
        )
        | (IPPROTO_TCP, TCP_NODELAY) => {
            log_dbg!(
                "Ignoring setsockopt({}, {:#x}, {:#x}, ...)",
                socket,
                level,
                option_name
            );
        }
        // A real device accepts any valid option, so don't fail.
        _ => {
            log!(
                "Warning: Ignoring unknown setsockopt({}, {:#x}, {:#x}, ...)",
                socket,
                level,
                option_name
            );
        }
    }

    0 // Success
}

/// Write an address to an app's `struct sockaddr` buffer and its length, as
/// for getsockname(), accept() and recvfrom(). The length is the buffer's size
/// on input: as much of the address as fits is written, and on output it is
/// the address's real size, like a real device.
fn write_sockaddr(
    env: &mut Environment,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
    value: sockaddr,
) {
    let size = guest_size_of::<sockaddr>();
    let buffer_size = env.mem.read(address_len);
    if buffer_size >= size {
        env.mem.write(address, value);
    } else {
        let bytes: [u8; 16] = unsafe { std::mem::transmute(value) };
        env.mem
            .bytes_at_mut(address.cast(), buffer_size)
            .copy_from_slice(&bytes[..buffer_size as usize]);
    }
    env.mem.write(address_len, size);
}

/// Check the length of an address an app passes in. Apps may pass a bigger
/// buffer, such as a `struct sockaddr_storage`, which is fine as long as it
/// holds an IPv4 address.
fn assert_sockaddr_len(address_len: socklen_t) {
    assert!(
        address_len >= guest_size_of::<sockaddr>(),
        "address length {address_len} is too short for an IPv4 address"
    );
}

fn getsockname(
    env: &mut Environment,
    socket: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    // The host socket for whatever state the socket is in, if any.
    let local_addr = if let Some(udp_socket) = &socket_host_object.udp_socket {
        Some(udp_socket.local_addr())
    } else if let Some(tcp_stream) = &socket_host_object.tcp_stream {
        Some(tcp_stream.local_addr())
    } else if let Some(tcp_stream) = &socket_host_object.pending_tcp_stream {
        Some(tcp_stream.local_addr())
    } else {
        socket_host_object
            .tcp_listener
            .as_ref()
            .map(|tcp_listener| tcp_listener.local_addr())
    };
    let local_addr = match local_addr {
        Some(Ok(SocketAddr::V4(local_addr))) => local_addr,
        // A socket that isn't bound or connected yet has no address. Like a
        // real device, report 0.0.0.0, port 0.
        None => SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, 0),
        Some(Ok(SocketAddr::V6(_))) => unreachable!(), // Only IPv4 is supported.
        Some(Err(error)) => panic!("getsockname: socket {socket} encountered IO error: {error}"),
    };
    let local_guest_addr = sockaddr::from_sockaddr_v4(&SocketAddr::V4(local_addr));
    write_sockaddr(env, address, address_len, local_guest_addr);

    0 // Success
}

fn getpeername(
    env: &mut Environment,
    socket: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> i32 {
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    // Only a connected socket has a peer: not a listener, and not a UDP
    // socket that connect() wasn't called for.
    let peer_addr = if let Some(tcp_stream) = &socket_host_object.tcp_stream {
        Some(tcp_stream.peer_addr())
    } else {
        socket_host_object
            .udp_socket
            .as_ref()
            .map(|udp_socket| udp_socket.peer_addr())
    };
    let peer_addr = match peer_addr {
        Some(Ok(peer_addr @ SocketAddr::V4(_))) => peer_addr,
        Some(Ok(SocketAddr::V6(_))) => unreachable!(), // Only IPv4 is supported.
        Some(Err(error)) if error.kind() != io::ErrorKind::NotConnected => {
            panic!("getpeername: socket {socket} encountered IO error: {error}")
        }
        None | Some(Err(_)) => {
            set_errno(env, ENOTCONN);
            return -1;
        }
    };
    let peer_guest_addr = sockaddr::from_sockaddr_v4(&peer_addr);
    write_sockaddr(env, address, address_len, peer_guest_addr);

    0 // Success
}

fn bind(
    env: &mut Environment,
    socket: i32,
    address: ConstPtr<sockaddr>,
    address_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    assert_sockaddr_len(address_len);
    let sockaddr_val = env.mem.read(address);
    log_dbg!(
        "bind({}, {:?} ({:?}), {})",
        socket,
        address,
        sockaddr_val,
        address_len
    );

    let mut socket_address = sockaddr_val.to_sockaddr_v4();
    if !env.options.network_access && !socket_address.ip().is_loopback() {
        // Only on this machine, so nothing is exposed to the network.
        log_dbg!(
            "bind: network access is off, binding to localhost instead of {}",
            socket_address.ip()
        );
        socket_address.set_ip(std::net::Ipv4Addr::LOCALHOST);
    }
    let type_str = match type_ {
        SOCK_STREAM => "TCP",
        SOCK_DGRAM => "UDP",
        _ => unreachable!(),
    };
    log_dbg!("bind: {} socket address {:?}", type_str, socket_address);

    // re-borrow
    let socket_host_object = State::get(env).sockets.get(&socket).unwrap();
    match type_ {
        SOCK_STREAM => {
            assert!(socket_host_object.tcp_listener.is_none());
            let host_socket = TcpListener::bind(socket_address).unwrap();
            // We set host socket as non-blocking in order to have
            // more control of how and when it's used
            host_socket.set_nonblocking(true).unwrap();
            // TODO: set options
            State::get_mut(env)
                .sockets
                .get_mut(&socket)
                .unwrap()
                .tcp_listener = Some(host_socket);
        }
        SOCK_DGRAM => {
            assert!(socket_host_object.udp_socket.is_none());
            let host_socket = UdpSocket::bind(socket_address).unwrap();
            // We set host socket as non-blocking in order to have
            // more control of how and when it's used
            host_socket.set_nonblocking(true).unwrap();
            for &option in &socket_host_object.options {
                if option == SO_BROADCAST {
                    host_socket.set_broadcast(true).unwrap();
                }
            }
            State::get_mut(env)
                .sockets
                .get_mut(&socket)
                .unwrap()
                .udp_socket = Some(host_socket);
        }
        _ => unreachable!(),
    }

    0 // Success
}

fn listen(env: &mut Environment, socket: i32, backlog: i32) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM);

    log!(
        "Warning: listen(socket: {}, backlog: {}), ignoring",
        socket,
        backlog
    );
    0 // Success
}

fn connect(
    env: &mut Environment,
    socket: i32,
    address: ConstPtr<sockaddr>,
    address_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM);

    assert_sockaddr_len(address_len);
    let sockaddr_val = env.mem.read(address);
    log_dbg!(
        "connect({:?} ({:?}), {})",
        address,
        sockaddr_val,
        address_len
    );

    let socket_address = sockaddr_val.to_sockaddr_v4();
    log_dbg!("connect: socket address {:?}", socket_address);

    if is_offline_destination(env, &socket_address) {
        log!(
            "Network access is off, connect({}) to {} fails with ENETUNREACH",
            socket,
            socket_address
        );
        set_errno(env, ENETUNREACH);
        return -1;
    }

    assert!(State::get(env)
        .sockets
        .get(&socket)
        .unwrap()
        .tcp_stream
        .is_none());
    let host_stream =
        match TcpStream::connect_timeout(&SocketAddr::V4(socket_address), CONNECT_TIMEOUT) {
            Ok(host_stream) => host_stream,
            Err(error) => {
                // Report the failure to the app, as a real device would.
                let errno = match error.kind() {
                    io::ErrorKind::ConnectionRefused => ECONNREFUSED,
                    io::ErrorKind::HostUnreachable => EHOSTUNREACH,
                    io::ErrorKind::NetworkUnreachable => ENETUNREACH,
                    _ => ETIMEDOUT,
                };
                log!(
                    "Warning: connect({}) to {} failed with {}, returning -1",
                    socket,
                    socket_address,
                    error
                );
                set_errno(env, errno);
                return -1;
            }
        };
    // We set host socket as non-blocking in order to have
    // more control of how and when it's used
    host_stream.set_nonblocking(true).unwrap();
    State::get_mut(env)
        .sockets
        .get_mut(&socket)
        .unwrap()
        .tcp_stream = Some(host_stream);

    0 // Success
}

fn select(
    env: &mut Environment,
    n_fds: i32,
    read_fds: MutPtr<fd_set>,
    write_fds: MutPtr<fd_set>,
    error_fds: MutPtr<fd_set>,
    timeout: MutPtr<timeval>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    if !(0..=FD_SETSIZE).contains(&n_fds) {
        set_errno(env, EINVAL);
        return -1;
    }

    if n_fds == 0 {
        // Apparently, this is a portable way of using select() as usleep()
        // https://stackoverflow.com/questions/3125645/why-use-select-instead-of-sleep
        // ¯\_(ツ)_/¯
        assert!(read_fds.is_null());
        assert!(write_fds.is_null());
        assert!(error_fds.is_null());
        let timeval = env.mem.read(timeout);
        let duration = Duration::from_secs(timeval.tv_sec.try_into().unwrap())
            + Duration::from_micros(timeval.tv_usec.try_into().unwrap());
        log_dbg!("select() used as sleep for {:?}", duration);
        env.sleep(duration);
        return 0;
    }

    // Checks every millisecond, letting other threads run, until something
    // is ready or the timeout (if any) is over.
    let deadline = (!timeout.is_null()).then(|| {
        let timeval = env.mem.read(timeout);
        let duration = Duration::from_secs(timeval.tv_sec.try_into().unwrap())
            + Duration::from_micros(timeval.tv_usec.try_into().unwrap());
        Instant::now() + duration
    });
    let set_ptrs = [read_fds, write_fds, error_fds];
    let sets = set_ptrs.map(|set| (!set.is_null()).then(|| env.mem.read(set)));
    loop {
        for (ptr, set) in set_ptrs.into_iter().zip(sets) {
            if let Some(set) = set {
                env.mem.write(ptr, set);
            }
        }
        let count = check_fds(env, n_fds, read_fds, write_fds, error_fds);
        if count > 0 || deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return count;
        }
        env.sleep(Duration::from_millis(1));
    }
}

/// Helper for [select]: checks each file descriptor once, leaving only the
/// ready ones in the sets.
fn check_fds(
    env: &mut Environment,
    n_fds: i32,
    read_fds: MutPtr<fd_set>,
    write_fds: MutPtr<fd_set>,
    error_fds: MutPtr<fd_set>,
) -> i32 {
    let mut count = 0;

    if !read_fds.is_null() {
        let mut read_set = env.mem.read(read_fds);
        log_dbg!("select: read_set before {:?}", read_set);
        count += process_set(env, &mut read_set, n_fds, |env, fd, bits, bit_index| {
            log_dbg!("select: bit set in read_set at fd: {}", fd);
            if let Some(ready) = pipe_is_ready(env, fd) {
                if !ready {
                    *bits &= !(1 << bit_index);
                }
                return ready;
            }
            assert!(is_socket(env, fd));
            // Clean bit in the set for the current socket
            *bits &= !(1 << bit_index);
            let socket_host_object = State::get(env).sockets.get(&fd).unwrap();
            let type_ = socket_host_object.type_;
            match type_ {
                SOCK_DGRAM => {
                    let udp_socket = socket_host_object.udp_socket.as_ref().unwrap();
                    // Peek just one byte to check if we have some data
                    let mut buf = [0; 1];
                    match udp_socket.peek(&mut buf) {
                        Ok(received) => {
                            log_dbg!("select: Socket {} peeked {} bytes", fd, received);
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        // On Windows, if we receive more bytes than we peek,
                        // it will error, but it means that there is some data!
                        Err(ref e)
                            if cfg!(target_os = "windows") && e.raw_os_error() == Some(10040) =>
                        {
                            // 10040 code is WSAEMSGSIZE
                            log_dbg!(
                                "[Windows case] select: received {} bytes (at least)",
                                buf.len()
                            );
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            log_dbg!("select: Socket {} would block on peeking, continue.", fd);
                            false
                        }
                        Err(e) => {
                            panic!("select: Peek for socket {fd} failed: {e:?}")
                        }
                    }
                }
                SOCK_STREAM => {
                    if socket_host_object.tcp_stream.is_none() {
                        // If we don't have a TCP stream it probably means
                        // that a listener is waiting for connection
                        let listener = socket_host_object.tcp_listener.as_ref().unwrap();
                        // The listener is non-blocking,
                        // so we can try to accept
                        match listener.accept() {
                            Ok((stream, addr)) => {
                                log!("select: New client: {}", addr);
                                // We set host socket as non-blocking in order
                                // to have more control of how and when it's
                                // used
                                stream.set_nonblocking(true).unwrap();
                                // We already accepted the connection on
                                // the host, but we need to postpone new
                                // guest fd creation up until guest calls
                                // accept()
                                assert!(socket_host_object.pending_tcp_stream.is_none());
                                State::get_mut(env)
                                    .sockets
                                    .get_mut(&fd)
                                    .unwrap()
                                    .pending_tcp_stream = Some(stream);
                                // Set bit back
                                *bits |= 1 << bit_index;
                                return true;
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                // No incoming connection is ready
                                log_dbg!("select: TCP listener for socket {} would block on accepting, continue.", fd);
                                return false;
                            }
                            Err(e) => {
                                panic!("select: Socket {fd} has error accepting connection: {e}");
                            }
                        }
                    }
                    let stream = socket_host_object.tcp_stream.as_ref().unwrap();
                    // Peek just one byte to check if we have some data
                    let mut buf = [0; 1];
                    match stream.peek(&mut buf) {
                        Ok(received) => {
                            log_dbg!("select: received {} bytes (at least)", received);
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        // On Windows, if we receive more bytes than we peek,
                        // it will error, but it means that there is some data!
                        Err(ref e)
                            if cfg!(target_os = "windows") && e.raw_os_error() == Some(10040) =>
                        {
                            // 10040 code is WSAEMSGSIZE
                            log_dbg!(
                                "[Windows case] select: received {} bytes (at least)",
                                buf.len()
                            );
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        // As tested on macOS, this marks socket as readable
                        Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                            log!("select: Peek for socket {}: ConnectionReset", fd);
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            log_dbg!(
                                "select: TCP stream for socket {} would block on peeking, continue.",
                                fd
                            );
                            false
                        }
                        Err(e) => {
                            panic!("select: Peek for socket {fd} failed: {e}")
                        }
                    }
                }
                _ => unimplemented!(),
            }
        });
        log_dbg!("select: read_set after {:?}", read_set);
        env.mem.write(read_fds, read_set);
    }

    if !write_fds.is_null() {
        let mut write_set = env.mem.read(write_fds);
        log_dbg!("select: write_set before {:?}", write_set);
        count += process_set(env, &mut write_set, n_fds, |env, fd, bits, bit_index| {
            log_dbg!("select: bit set in write_set at fd: {}", fd);
            if let Some(ready) = pipe_is_ready(env, fd) {
                if !ready {
                    *bits &= !(1 << bit_index);
                }
                return ready;
            }
            assert!(is_socket(env, fd));
            // Clean bit in the set for the current socket
            *bits &= !(1 << bit_index);
            let socket_host_object = State::get(env).sockets.get(&fd).unwrap();
            let type_ = socket_host_object.type_;
            match type_ {
                SOCK_STREAM => {
                    assert!(socket_host_object.tcp_listener.is_none());
                    // As we cannot "peek" on write, we just check
                    // if TCP stream exist or not
                    // TODO: find a better way
                    if socket_host_object.tcp_stream.is_some() {
                        // Set bit back
                        *bits |= 1 << bit_index;
                        true
                    } else {
                        false
                    }
                }
                SOCK_DGRAM => {
                    // As we cannot "peek" on write, we just check
                    // if UDP socket exist or not
                    // TODO: find a better way
                    if socket_host_object.udp_socket.is_some() {
                        // Set bit back
                        *bits |= 1 << bit_index;
                        true
                    } else {
                        false
                    }
                }
                _ => unimplemented!(),
            }
        });
        log_dbg!("select: write_set after {:?}", write_set);
        env.mem.write(write_fds, write_set);
    }

    if !error_fds.is_null() {
        let mut error_set = env.mem.read(error_fds);
        log_dbg!("select: error_set before {:?}", error_set);
        count += process_set(env, &mut error_set, n_fds, |env, fd, bits, bit_index| {
            log_dbg!("select: bit set in error_set at fd: {}", fd);
            // Pipes have no exceptional conditions.
            if pipe_is_ready(env, fd).is_some() {
                *bits &= !(1 << bit_index);
                return false;
            }
            assert!(is_socket(env, fd));
            // Clean bit in the set for the current socket
            *bits &= !(1 << bit_index);
            let socket_host_object = State::get(env).sockets.get(&fd).unwrap();
            let type_ = socket_host_object.type_;
            match type_ {
                SOCK_STREAM => {
                    assert!(socket_host_object.tcp_listener.is_none());
                    let stream = socket_host_object.tcp_stream.as_ref().unwrap();
                    match stream.take_error() {
                        Ok(None) => {
                            log_dbg!("No error on TCP socket {}", fd);
                            false
                        }
                        Ok(Some(error)) => unimplemented!("TCP socket {} error: {:?}", fd, error),
                        Err(error) => panic!("TCP socket {fd} take_error failed: {error:?}"),
                    }
                }
                SOCK_DGRAM => {
                    todo!()
                }
                _ => unimplemented!(),
            }
        });
        log_dbg!("select: error_set after {:?}", error_set);
        env.mem.write(error_fds, error_set);
    }

    count
}

fn process_set<F: Fn(&mut Environment, FileDescriptor, &mut i32, i32) -> bool>(
    env: &mut Environment,
    set: &mut fd_set,
    n_fds: i32,
    process_bit: F,
) -> i32 {
    let mut fds_bits = set.fds_bits;
    let mut count = 0;
    'outer: for (i, bits) in fds_bits.iter_mut().enumerate() {
        for bit_index in 0..32i32 {
            let fd: FileDescriptor = (i as i32) * 32 + bit_index;
            if fd > n_fds {
                break 'outer;
            }
            if (*bits & (1 << bit_index)) != 0 && process_bit(env, fd, bits, bit_index) {
                count += 1;
            }
        }
    }
    set.fds_bits = fds_bits;
    count
}

fn accept(
    env: &mut Environment,
    socket: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> FileDescriptor {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM);

    if let Some(stream) = State::get_mut(env)
        .sockets
        .get_mut(&socket)
        .unwrap()
        .pending_tcp_stream
        .take()
    {
        let addr = stream.peer_addr().unwrap();
        // We have already accepted TCP socket, we now need to
        // let guest know as well!
        let new_fd = find_or_create_socket(env);
        assert!(!State::get(env).sockets.contains_key(&new_fd));
        let host_object = SocketHostObject {
            type_: SOCK_STREAM,
            options: Default::default(),
            tcp_listener: None,
            pending_tcp_stream: None,
            tcp_stream: Some(stream),
            udp_socket: None,
        };
        State::get_mut(env).sockets.insert(new_fd, host_object);
        assert!(!address.is_null());
        let peer_guest_addr = sockaddr::from_sockaddr_v4(&addr);
        write_sockaddr(env, address, address_len, peer_guest_addr);
        return new_fd;
    }

    // re-borrow
    let socket_host_object = State::get(env).sockets.get(&socket).unwrap();
    let listener = socket_host_object.tcp_listener.as_ref().unwrap();
    match listener.accept() {
        Ok((_, addr)) => {
            log!("accept: New client: {}", addr);
            unimplemented!()
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            if is_nonblocking(env, socket) {
                // What a non-blocking socket is for: try again later.
                set_errno(env, EAGAIN);
                return -1;
            }
            // No incoming connection is ready
            // TODO: if this happened, take a deep breath and do:
            // - block guest thread with a new [ThreadBlock] type
            // - poll for data in thread scheduling part
            // - write/read/accept/etc data once it is ready
            // - unblock guest thread
            unimplemented!("accept: TCP listener for socket {} would block on accepting, block current guest thread {}.", socket, env.current_thread)
        }
        Err(e) => {
            panic!("accept: Socket {socket} has error accepting connection: {e}");
        }
    }
}

fn recv(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
) -> i32 {
    recvfrom(env, socket, buffer, length, flags, Ptr::null(), Ptr::null())
}

fn recvfrom(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "recvfrom({}, {:?}, {}, {}, {:?}, {:?})",
        socket,
        buffer,
        length,
        flags,
        address,
        address_len
    );

    if !State::get(env).sockets.contains_key(&socket) {
        set_errno(env, EBADF);
        log!(
            "Warning: recvfrom({}, ...) failed for unknown socket, returning -1",
            socket
        );
        return -1;
    }

    let type_ = State::get(env).sockets.get(&socket).unwrap().type_;
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    // MSG_TRUNC only means something as an input flag on Linux, and iOS ignores
    // it, so cross-platform code may pass it. MSG_DONTWAIT makes this call
    // non-blocking.
    assert!(
        flags & !(MSG_TRUNC | MSG_DONTWAIT) == 0,
        "recvfrom: unsupported flags {flags:#x}"
    );
    let dont_wait = flags & MSG_DONTWAIT != 0;

    let (num_bytes_read, addr) = match type_ {
        SOCK_DGRAM => {
            let udp_socket = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .udp_socket
                .as_ref()
                .unwrap();
            let buf = env.mem.bytes_at_mut(buffer.cast(), length);
            let (read, addr) = match udp_socket.recv_from(buf) {
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if dont_wait || is_nonblocking(env, socket) {
                        // What a non-blocking socket is for: try again later.
                        set_errno(env, EAGAIN);
                        return -1;
                    }
                    // No data is ready
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    unimplemented!("recvfrom: UDP socket {} would block on receiving, block current guest thread {}.", socket, env.current_thread)
                }
                Err(e) => panic!("recvfrom: UDP socket {socket} encountered IO error: {e}"),
            };
            if !address.is_null() {
                let guest_addr = sockaddr::from_sockaddr_v4(&addr);
                write_sockaddr(env, address, address_len, guest_addr);
            }
            (read, Ok(addr))
        }
        SOCK_STREAM => {
            assert!(address.is_null());
            assert!(address_len.is_null());
            let mut tcp_stream = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .tcp_stream
                .as_ref()
                .unwrap();
            let buf = env.mem.bytes_at_mut(buffer.cast(), length);
            let read = match tcp_stream.read(buf) {
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    set_errno(env, ECONNRESET);
                    log!("recvfrom: TCP socket {}: ConnectionReset => -1", socket);
                    return -1;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if dont_wait || is_nonblocking(env, socket) {
                        // What a non-blocking socket is for: try again later.
                        set_errno(env, EAGAIN);
                        return -1;
                    }
                    // No data is ready
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    unimplemented!("recvfrom: TCP socket {} would block on receiving, block current guest thread {}.", socket, env.current_thread)
                }
                Err(e) => panic!("recvfrom: TCP socket {socket} encountered IO error: {e}"),
            };
            (read, tcp_stream.peer_addr())
        }
        _ => unreachable!(),
    };
    log_dbg!(
        "recvfrom: Socket {} received {} bytes from addr {:?}",
        socket,
        num_bytes_read,
        addr.ok()
    );
    num_bytes_read.try_into().unwrap()
}

fn send(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let type_ = State::get(env).sockets.get(&socket).unwrap().type_;
    assert!(type_ == SOCK_STREAM);

    assert_eq!(flags, 0); // TODO

    let num_bytes_written = match type_ {
        SOCK_STREAM => {
            let mut tcp_stream = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .tcp_stream
                .as_ref()
                .unwrap();
            let buf = env.mem.bytes_at(buffer.cast(), length);
            match tcp_stream.write(buf) {
                Ok(written) => written,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if is_nonblocking(env, socket) {
                        // What a non-blocking socket is for: try again later.
                        set_errno(env, EAGAIN);
                        return -1;
                    }
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    unimplemented!("send: TCP socket {} would block on sending, block current guest thread {}.", socket, env.current_thread)
                }
                Err(e) => panic!("send: Socket {socket} encountered IO error: {e}"),
            }
        }
        _ => unreachable!(),
    };
    log_dbg!(
        "send: written {} bytes to TCP socket {}",
        num_bytes_written,
        socket
    );
    num_bytes_written.try_into().unwrap()
}

fn sendto(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
    dest_address: MutPtr<sockaddr>,
    dest_address_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let type_ = State::get(env).sockets.get(&socket).unwrap().type_;
    assert!(type_ == SOCK_DGRAM);

    assert_eq!(flags, 0); // TODO

    assert_sockaddr_len(dest_address_len);
    let sockaddr_val = env.mem.read(dest_address);
    let socket_address = sockaddr_val.to_sockaddr_v4();
    log_dbg!(
        "sendto({}, {:?}, {}, {}, {:?} ({:?}, {:?}), {})",
        socket,
        buffer,
        length,
        flags,
        dest_address,
        sockaddr_val,
        socket_address,
        dest_address_len
    );

    if is_offline_destination(env, &socket_address) {
        log_dbg!(
            "Network access is off, sendto({}) to {} fails with ENETUNREACH",
            socket,
            socket_address
        );
        set_errno(env, ENETUNREACH);
        return -1;
    }

    let num_bytes_written = match type_ {
        SOCK_DGRAM => {
            if State::get(env)
                .sockets
                .get(&socket)
                .unwrap()
                .udp_socket
                .is_none()
            {
                // For the case of broadcast we allow a lazy host UDP socket
                // creation
                assert!(socket_address.ip().is_broadcast());
                // TODO: is it a correct address to bind?
                let host_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
                assert!(host_socket.local_addr().unwrap().ip().is_unspecified());
                // We set host socket as non-blocking in order to have
                // more control of how and when it's used
                host_socket.set_nonblocking(true).unwrap();
                for &option in &State::get(env).sockets.get(&socket).unwrap().options {
                    if option == SO_BROADCAST {
                        host_socket.set_broadcast(true).unwrap();
                    }
                }
                assert!(host_socket.broadcast().unwrap());
                State::get_mut(env)
                    .sockets
                    .get_mut(&socket)
                    .unwrap()
                    .udp_socket = Some(host_socket);
            }
            let udp_socket = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .udp_socket
                .as_ref()
                .unwrap();
            if socket_address.ip().is_broadcast() {
                assert!(udp_socket.local_addr().unwrap().ip().is_unspecified());
            }
            let buf = env.mem.bytes_at(buffer.cast(), length);
            match udp_socket.send_to(buf, socket_address) {
                Ok(written) => written,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if is_nonblocking(env, socket) {
                        // What a non-blocking socket is for: try again later.
                        set_errno(env, EAGAIN);
                        return -1;
                    }
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    unimplemented!("sendto: UDP socket {} would block on sending, block current guest thread {}.", socket, env.current_thread)
                }
                Err(e) => panic!("sendto: Socket {socket} encountered IO error: {e}"),
            }
        }
        _ => unreachable!(),
    };
    log_dbg!(
        "sendto: written {} bytes to UDP socket {} (address {:?})",
        num_bytes_written,
        socket,
        socket_address
    );
    num_bytes_written.try_into().unwrap()
}

const SHUT_RDWR: i32 = 2;
fn shutdown(env: &mut Environment, socket: i32, how: i32) -> i32 {
    log_dbg!("shutdown({}, {})", socket, how);
    assert_eq!(how, SHUT_RDWR);
    close(env, socket)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(socket(_, _, _)),
    export_c_func!(ioctl(_, _, _)),
    export_c_func!(getsockopt(_, _, _, _, _)),
    export_c_func!(setsockopt(_, _, _, _, _)),
    export_c_func!(getsockname(_, _, _)),
    export_c_func!(getpeername(_, _, _)),
    export_c_func!(bind(_, _, _)),
    export_c_func!(listen(_, _)),
    export_c_func!(connect(_, _, _)),
    export_c_func!(select(_, _, _, _, _)),
    export_c_func!(accept(_, _, _)),
    export_c_func!(recv(_, _, _, _)),
    export_c_func!(recvfrom(_, _, _, _, _, _)),
    export_c_func!(send(_, _, _, _)),
    export_c_func!(sendto(_, _, _, _, _, _)),
    export_c_func!(shutdown(_, _)),
];

/// A helper to close a socket, not a part of API
pub fn close_socket(env: &mut Environment, socket: i32) -> bool {
    State::get_mut(env).sockets.remove(&socket).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_failure_is_reported_to_the_app() {
        let mut env = Environment::new_fake_for_test();
        env.options.network_access = true;

        // A port nothing is listening on.
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };

        let fd = socket(&mut env, AF_INET, SOCK_STREAM, 0);
        assert!(fd >= 0);
        let address = env
            .mem
            .alloc_and_write(sockaddr::from_ipv4_parts([127, 0, 0, 1], port));
        let result = connect(
            &mut env,
            fd,
            address.cast_const(),
            guest_size_of::<sockaddr>(),
        );
        assert_eq!(result, -1);
        assert_eq!(crate::libc::errno::get_errno(&mut env), ECONNREFUSED);

        // It has no address, as it isn't bound or connected.
        let name = env
            .mem
            .alloc_and_write(sockaddr::from_ipv4_parts([1, 2, 3, 4], 5));
        let name_len = env.mem.alloc_and_write(128 as socklen_t);
        assert_eq!(getsockname(&mut env, fd, name, name_len), 0);
        let name = env.mem.read(name);
        assert_eq!(
            name.to_sockaddr_v4(),
            SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, 0)
        );
        assert_eq!(env.mem.read(name_len), guest_size_of::<sockaddr>());
    }

    #[test]
    fn sockets_work_as_in_airplane_mode_without_network_access() {
        let mut env = Environment::new_fake_for_test();
        assert!(!env.options.network_access);
        let addr_len = guest_size_of::<sockaddr>();

        // UDP: can be created and bound, but only to this machine.
        let udp = socket(&mut env, AF_INET, SOCK_DGRAM, 0);
        assert!(udp >= 0);
        let any = env
            .mem
            .alloc_and_write(sockaddr::from_ipv4_parts([0, 0, 0, 0], 0));
        assert_eq!(bind(&mut env, udp, any.cast_const(), addr_len), 0);
        let name = env.mem.alloc(addr_len).cast();
        let name_len = env.mem.alloc_and_write(addr_len);
        assert_eq!(getsockname(&mut env, udp, name, name_len), 0);
        assert!(env.mem.read(name).to_sockaddr_v4().ip().is_loopback());

        // Sending elsewhere fails, and nothing arrives.
        let elsewhere = env
            .mem
            .alloc_and_write(sockaddr::from_ipv4_parts([10, 0, 0, 1], 9));
        let buffer = env.mem.alloc(16);
        assert_eq!(
            sendto(&mut env, udp, buffer, 16, 0, elsewhere, addr_len),
            -1
        );
        assert_eq!(crate::libc::errno::get_errno(&mut env), ENETUNREACH);
        assert_eq!(
            recvfrom(
                &mut env,
                udp,
                buffer,
                16,
                MSG_DONTWAIT,
                Ptr::null(),
                Ptr::null()
            ),
            -1
        );
        assert_eq!(crate::libc::errno::get_errno(&mut env), EAGAIN);

        // TCP: connecting elsewhere fails straight away.
        let tcp = socket(&mut env, AF_INET, SOCK_STREAM, 0);
        assert!(tcp >= 0);
        assert_eq!(connect(&mut env, tcp, elsewhere.cast_const(), addr_len), -1);
        assert_eq!(crate::libc::errno::get_errno(&mut env), ENETUNREACH);

        // But connecting to this machine works, as in airplane mode.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let local = env
            .mem
            .alloc_and_write(sockaddr::from_ipv4_parts([127, 0, 0, 1], port));
        let tcp = socket(&mut env, AF_INET, SOCK_STREAM, 0);
        assert_eq!(connect(&mut env, tcp, local.cast_const(), addr_len), 0);
    }
}
