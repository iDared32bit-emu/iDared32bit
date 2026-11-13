/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Objective-C exceptions (`@throw`, `@try`/`@catch`).
//!
//! As in Apple's runtime, they are C++ exceptions underneath: thrown with the
//! app's own libstdc++ (`__cxa_throw`), unwound by its libgcc, and matched by
//! `__gxx_personality_sj0` (which `__objc_personality_v0` is linked to).
//! What makes them Objective-C is the type info: its `__do_catch` matches by
//! class. Apps without libstdc++ can't catch them, so throwing panics there.

use super::{id, msg, msg_class, nil, release, retain, Class};
use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::frameworks::foundation::ns_string::to_rust_string;
use crate::mem::{ConstPtr, ConstVoidPtr, GuestUSize, MutPtr, MutVoidPtr, Ptr};
use crate::Environment;

/// The exception object `__cxa_throw` throws: the Objective-C object, then
/// its type info (vtable, name, class), as Apple lays it out.
const EXCEPTION_SIZE: GuestUSize = 16;

/// A function the app's own libraries export, e.g. libstdc++'s.
fn guest_function(env: &Environment, symbol: &str) -> Option<GuestFunction> {
    env.bins
        .iter()
        .find_map(|bin| bin.exported_symbols.get(symbol))
        .map(|&addr| GuestFunction::from_addr_with_thumb_bit(addr))
}

fn host_function(env: &mut Environment, symbol: &str) -> u32 {
    env.dyld
        .create_proc_address(&mut env.mem, &mut env.cpu, symbol)
        .unwrap()
        .addr_with_thumb_bit()
}

/// The type info vtable (as in C++), allocated once: offset and RTTI (0),
/// two destructors, `__is_pointer_p`, `__is_function_p`, `__do_catch`,
/// `__do_upcast`. Type infos point 8 bytes in, past the first two.
fn ehtype_vtable(env: &mut Environment) -> u32 {
    if let Some(vtable) = env.objc.ehtype_vtable {
        return vtable;
    }
    let noop = host_function(env, "__touchHLE_objc_ehtype_noop");
    let false_ = host_function(env, "__touchHLE_objc_ehtype_false");
    let do_catch = host_function(env, "__touchHLE_objc_ehtype_do_catch");
    let entries: [u32; 8] = [0, 0, noop, noop, false_, false_, do_catch, false_];
    let vtable: MutPtr<u32> = env.mem.alloc(8 * 4).cast();
    for (i, entry) in entries.into_iter().enumerate() {
        env.mem.write(vtable + i as GuestUSize, entry);
    }
    let vtable = vtable.to_bits() + 8;
    env.objc.ehtype_vtable = Some(vtable);
    vtable
}

/// A type info for catching a class (`nil` catches every object).
fn make_ehtype(env: &mut Environment, name: &str, class: Class) -> ConstVoidPtr {
    let vtable = ehtype_vtable(env);
    let name = env.mem.alloc_and_write_cstr(name.as_bytes()).to_bits();
    let ehtype: MutPtr<u32> = env.mem.alloc(12).cast();
    env.mem.write(ehtype, vtable);
    env.mem.write(ehtype + 1, name);
    env.mem.write(ehtype + 2, class.to_bits());
    ehtype.cast().cast_const()
}

fn _touchHLE_objc_ehtype_noop(_env: &mut Environment, _this: MutVoidPtr) {}
fn _touchHLE_objc_ehtype_false(_env: &mut Environment, _this: MutVoidPtr) -> bool {
    false
}

/// Whether a `@catch` (`catch_type`) matches the thrown object.
fn _touchHLE_objc_ehtype_do_catch(
    env: &mut Environment,
    catch_type: ConstPtr<u32>,
    throw_type: ConstPtr<u32>,
    thrown_ptr_ptr: ConstPtr<ConstPtr<id>>,
    _outer: u32,
) -> bool {
    // Only Objective-C exceptions, not C++ ones, are caught by @catch.
    let vtable = ehtype_vtable(env);
    if env.mem.read(throw_type) != vtable {
        return false;
    }
    let exception: id = env.mem.read(env.mem.read(thrown_ptr_ptr));
    let class: Class = Ptr::from_bits(env.mem.read(catch_type + 2));
    class == nil || msg![env; exception isKindOfClass:class]
}

/// Called when libstdc++ frees the exception object.
fn _touchHLE_objc_exception_destructor(env: &mut Environment, exception: MutPtr<u32>) {
    let object: id = Ptr::from_bits(env.mem.read(exception));
    let name: MutVoidPtr = Ptr::from_bits(env.mem.read(exception + 2));
    env.mem.free(name);
    release(env, object);
}

pub fn objc_exception_throw(env: &mut Environment, exception: id) {
    // Any object can be thrown, not only an NSException.
    let ns_exception: Class = msg_class![env; NSException class];
    let (name, reason) = if msg![env; exception isKindOfClass:ns_exception] {
        let name: id = msg![env; exception name];
        let reason: id = msg![env; exception reason];
        (to_rust_string(env, name), to_rust_string(env, reason))
    } else {
        let class: Class = msg![env; exception class];
        let class_name = env.objc.get_class_name(class).to_owned();
        (class_name.into(), "not an NSException".into())
    };
    log!("Throwing Objective-C exception {} ({})", name, reason);

    let (Some(allocate), Some(throw)) = (
        guest_function(env, "___cxa_allocate_exception"),
        guest_function(env, "___cxa_throw"),
    ) else {
        panic!("Uncaught Objective-C exception {name} ({reason}): no libstdc++ to throw it with");
    };
    retain(env, exception);
    let exc: MutVoidPtr = allocate.call_from_host(env, (EXCEPTION_SIZE,));
    let exc: MutPtr<u32> = exc.cast();
    let class: Class = msg![env; exception class];
    let class_name = env.objc.get_class_name(class).to_owned();
    let vtable = ehtype_vtable(env);
    let class_name = env
        .mem
        .alloc_and_write_cstr(class_name.as_bytes())
        .to_bits();
    env.mem.write(exc, exception.to_bits());
    env.mem.write(exc + 1, vtable);
    env.mem.write(exc + 2, class_name);
    env.mem.write(exc + 3, class.to_bits());
    let destructor = host_function(env, "__touchHLE_objc_exception_destructor");
    // This only comes back once the guest has unwound past this call, to the
    // @catch; if nothing catches it, libstdc++ terminates.
    let _: () = throw.call_from_host(env, (exc, exc + 1, destructor));
    assert!(env.is_unwinding());
}

fn objc_exception_rethrow(env: &mut Environment) {
    let rethrow = guest_function(env, "___cxa_rethrow").unwrap();
    let _: () = rethrow.call_from_host(env, ());
    assert!(env.is_unwinding());
}

fn objc_begin_catch(env: &mut Environment, exception_buffer: MutVoidPtr) -> id {
    let begin = guest_function(env, "___cxa_begin_catch").unwrap();
    let exc: MutVoidPtr = begin.call_from_host(env, (exception_buffer,));
    env.mem.read(exc.cast::<id>())
}

fn objc_end_catch(env: &mut Environment) {
    let end = guest_function(env, "___cxa_end_catch").unwrap();
    let _: () = end.call_from_host(env, ());
}

pub const CONSTANTS: ConstantExports = &[
    (
        "_objc_ehtype_vtable",
        HostConstant::Custom(|env| {
            let vtable = ehtype_vtable(env);
            Ptr::from_bits(vtable - 8)
        }),
    ),
    (
        "_OBJC_EHTYPE_id",
        HostConstant::Custom(|env| make_ehtype(env, "id", nil)),
    ),
    (
        "_OBJC_EHTYPE_$_NSException",
        HostConstant::Custom(|env| {
            let class = env.objc.get_known_class("NSException", &mut env.mem);
            make_ehtype(env, "NSException", class)
        }),
    ),
];

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(objc_exception_throw(_)),
    export_c_func!(objc_exception_rethrow()),
    export_c_func!(objc_begin_catch(_)),
    export_c_func!(objc_end_catch()),
    export_c_func!(_touchHLE_objc_ehtype_noop(_)),
    export_c_func!(_touchHLE_objc_ehtype_false(_)),
    export_c_func!(_touchHLE_objc_ehtype_do_catch(_, _, _, _)),
    export_c_func!(_touchHLE_objc_exception_destructor(_)),
];
