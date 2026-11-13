/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `netdb.h`

use crate::dyld::FunctionExports;
use crate::export_c_func;
use crate::libc::sys::socket::{sockaddr, AF_INET, SOCK_DGRAM, SOCK_STREAM};
use crate::mem::{guest_size_of, ConstPtr, MutPtr, Ptr, SafeRead};
use crate::Environment;

const AI_PASSIVE: i32 = 0x1;

pub const IPPROTO_TCP: i32 = 6;
pub const IPPROTO_UDP: i32 = 17;

const EAI_FAIL: i32 = 4;

#[allow(non_camel_case_types)]
pub type socklen_t = u32;

#[derive(Default)]
pub struct State {
    /// Static storage for `gethostbyname`'s result, which the next call may
    /// overwrite: the [hostent] and what it points to.
    hostent_tmp: Option<MutPtr<u8>>,
}

#[repr(C, packed)]
#[allow(non_camel_case_types)]
struct hostent {
    h_name: MutPtr<u8>,
    h_aliases: MutPtr<MutPtr<u8>>,
    h_addrtype: i32,
    h_length: i32,
    h_addr_list: MutPtr<MutPtr<u8>>,
}
unsafe impl SafeRead for hostent {}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct addrinfo {
    ai_flags: i32,
    ai_family: i32,
    ai_socktype: i32,
    ai_protocol: i32,
    ai_addrlen: socklen_t,
    ai_canonname: MutPtr<u8>,
    ai_addr: MutPtr<sockaddr>,
    ai_next: MutPtr<addrinfo>,
}
unsafe impl SafeRead for addrinfo {}

fn getaddrinfo(
    env: &mut Environment,
    node_name: MutPtr<u8>,
    serv_name: MutPtr<u8>,
    hints: ConstPtr<addrinfo>,
    res: MutPtr<MutPtr<addrinfo>>,
) -> i32 {
    if !env.options.network_access {
        log_dbg!(
            "Network access is disabled, getaddrinfo({:?}, {:?}, {:?}, {:?}) -> EAI_FAIL",
            node_name,
            serv_name,
            hints,
            res
        );
        return EAI_FAIL;
    }

    assert!(node_name.is_null()); // TODO

    let hint = env.mem.read(hints);
    let ai_flags = hint.ai_flags;
    assert_eq!(ai_flags, AI_PASSIVE);
    let ai_family = hint.ai_family;
    assert_eq!(ai_family, AF_INET);
    assert!(hint.ai_socktype == SOCK_STREAM || hint.ai_socktype == SOCK_DGRAM);
    assert!(
        hint.ai_protocol == IPPROTO_TCP || hint.ai_protocol == IPPROTO_UDP || hint.ai_protocol == 0
    );
    let ai_addrlen = hint.ai_addrlen;
    assert_eq!(ai_addrlen, 0);
    assert!(hint.ai_canonname.is_null());
    assert!(hint.ai_addr.is_null());
    assert!(hint.ai_next.is_null());

    let mut addr_info = hint;
    let port: u16 = env.mem.cstr_at_utf8(serv_name).unwrap().parse().unwrap();
    log_dbg!("getaddrinfo: port {}", port);
    let addr = sockaddr::from_ipv4_parts([0; 4], port);

    let tmp_addr = env.mem.alloc_and_write(addr);
    addr_info.ai_addr = tmp_addr;
    addr_info.ai_addrlen = guest_size_of::<sockaddr>();

    let tmp_addr_info = env.mem.alloc_and_write(addr_info);
    env.mem.write(res, tmp_addr_info);

    0 // Success
}

fn freeaddrinfo(env: &mut Environment, addrinfo: MutPtr<addrinfo>) {
    let addrinfo_val = env.mem.read(addrinfo);
    assert!(addrinfo_val.ai_next.is_null()); // TODO
    let ai_addrlen = addrinfo_val.ai_addrlen;
    assert_eq!(ai_addrlen, guest_size_of::<sockaddr>());
    env.mem.free(addrinfo_val.ai_addr.cast());
    env.mem.free(addrinfo.cast());
}

fn gethostbyname(env: &mut Environment, name: ConstPtr<u8>) -> MutPtr<hostent> {
    let name_str = env.mem.cstr_at_utf8(name).unwrap();
    // A numeric address needs no lookup, so it works even in airplane mode.
    let Ok(ip) = name_str.parse::<std::net::Ipv4Addr>() else {
        log!("TODO: gethostbyname({:?} \"{}\") => NULL", name, name_str);
        // TODO: set h_errno
        return Ptr::null();
    };
    let name_str = ip.to_string();

    // The hostent, then: aliases (just NULL), addresses (one, then NULL),
    // the address, and the name (at most 16 bytes).
    const SIZE: u32 = 20 + 4 + 8 + 4 + 16;
    let base = *env
        .libc_state
        .netdb
        .hostent_tmp
        .get_or_insert_with(|| env.mem.alloc(SIZE).cast());
    let aliases: MutPtr<MutPtr<u8>> = (base + 20).cast();
    let addr_list: MutPtr<MutPtr<u8>> = (base + 24).cast();
    let addr = base + 32;
    let h_name = base + 36;
    env.mem.write(aliases, Ptr::null());
    env.mem.write(addr_list, addr);
    env.mem.write(addr_list + 1, Ptr::null());
    env.mem.bytes_at_mut(addr, 4).copy_from_slice(&ip.octets());
    let name_bytes = env.mem.bytes_at_mut(h_name, 16);
    name_bytes.fill(0);
    name_bytes[..name_str.len()].copy_from_slice(name_str.as_bytes());
    let entry = hostent {
        h_name,
        h_aliases: aliases,
        h_addrtype: AF_INET,
        h_length: 4,
        h_addr_list: addr_list,
    };
    let entry_ptr: MutPtr<hostent> = base.cast();
    env.mem.write(entry_ptr, entry);
    entry_ptr
}

fn gethostent(_env: &mut Environment) -> MutPtr<hostent> {
    log!("TODO: gethostent() => NULL");
    // TODO: set h_errno
    Ptr::null()
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(getaddrinfo(_, _, _, _)),
    export_c_func!(freeaddrinfo(_)),
    export_c_func!(gethostbyname(_)),
    export_c_func!(gethostent()),
];
