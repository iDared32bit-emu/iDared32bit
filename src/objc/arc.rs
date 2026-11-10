/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Runtime support for Automatic Reference Counting (ARC).
//!
//! Resources:
//! - [Objective-C Automatic Reference Counting, "Runtime support"](https://clang.llvm.org/docs/AutomaticReferenceCounting.html#runtime-support)
//!
//! The compiler calls these instead of sending retain, release and
//! autorelease. Weak references (iOS 5+) aren't supported yet.

use super::blocks::_Block_copy;
use super::{autorelease, id, msg, msg_class, nil, release, retain, ObjC};
use crate::abi::CallFromHost;
use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{MutPtr, MutVoidPtr};
use crate::Environment;

fn objc_retain(env: &mut Environment, object: id) -> id {
    retain(env, object)
}
fn objc_release(env: &mut Environment, object: id) {
    release(env, object)
}
fn objc_autorelease(env: &mut Environment, object: id) -> id {
    autorelease(env, object)
}
fn objc_retainAutorelease(env: &mut Environment, object: id) -> id {
    let object = retain(env, object);
    autorelease(env, object)
}
// The ReturnValue pair lets a callee and caller skip the autorelease pool
// between them. Doing it the long way is always correct.
fn objc_autoreleaseReturnValue(env: &mut Environment, object: id) -> id {
    autorelease(env, object)
}
fn objc_retainAutoreleaseReturnValue(env: &mut Environment, object: id) -> id {
    objc_retainAutorelease(env, object)
}
fn objc_retainAutoreleasedReturnValue(env: &mut Environment, object: id) -> id {
    retain(env, object)
}
fn objc_storeStrong(env: &mut Environment, location: MutPtr<id>, value: id) {
    let value = retain(env, value);
    let old = env.mem.read(location);
    env.mem.write(location, value);
    release(env, old);
}
fn objc_retainBlock(env: &mut Environment, block: id) -> id {
    _Block_copy(env, block.cast().cast_const())
        .cast_mut()
        .cast()
}

// @autoreleasepool { } blocks.
fn objc_autoreleasePoolPush(env: &mut Environment) -> MutVoidPtr {
    let pool: id = msg_class![env; NSAutoreleasePool new];
    pool.cast()
}
fn objc_autoreleasePoolPop(env: &mut Environment, pool: MutVoidPtr) {
    // Like draining an NSAutoreleasePool, this also pops any pools pushed
    // after it.
    let pool: id = pool.cast();
    () = msg![env; pool drain];
}

/// Calls the `.cxx_destruct` methods ARC generates to release an object's
/// instance variables, most-derived class first, as iOS does on dealloc.
pub fn call_cxx_destructors(env: &mut Environment, object: id) {
    let Some(sel) = env.objc.lookup_selector(".cxx_destruct") else {
        return;
    };
    let mut class = ObjC::read_isa(object, &env.mem);
    while class != nil {
        if env.objc.class_has_uninherited_method(class, sel) {
            let imp = ObjC::class_get_method_guest_function(env, class, sel);
            () = imp.call_from_host(env, (object, sel));
        }
        class = env.objc.get_superclass(class);
    }
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(objc_retain(_)),
    export_c_func!(objc_release(_)),
    export_c_func!(objc_autorelease(_)),
    export_c_func!(objc_retainAutorelease(_)),
    export_c_func!(objc_autoreleaseReturnValue(_)),
    export_c_func!(objc_retainAutoreleaseReturnValue(_)),
    export_c_func!(objc_retainAutoreleasedReturnValue(_)),
    export_c_func!(objc_storeStrong(_, _)),
    export_c_func!(objc_retainBlock(_)),
    export_c_func!(objc_autoreleasePoolPush()),
    export_c_func!(objc_autoreleasePoolPop(_)),
];
