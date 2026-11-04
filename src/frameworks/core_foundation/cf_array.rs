/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CFArray` and `CFMutableArray`.
//!
//! These are toll-free bridged to `NSArray` and `NSMutableArray` in Apple's
//! implementation. Here they are the same types.

use super::cf_allocator::{kCFAllocatorDefault, CFAllocatorRef};
use super::{CFIndex, CFRange};
use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::foundation::NSUInteger;
use crate::mem::{ConstVoidPtr, MutVoidPtr};
use crate::objc::{id, msg, msg_class, Class};
use crate::Environment;

#[allow(dead_code)]
pub type CFArrayRef = super::CFTypeRef;
pub type CFMutableArrayRef = super::CFTypeRef;

fn CFArrayCreateMutable(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    capacity: CFIndex,
    callbacks: ConstVoidPtr, // TODO, should be `const CFArrayCallBacks*`
) -> CFMutableArrayRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    assert!(capacity == 0); // TODO: fixed capacity support
    assert!(callbacks.is_null()); // TODO: support retaining etc

    msg_class![env; _touchHLE_NSMutableArray_non_retaining new]
}

fn CFArrayCreateMutableCopy(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    capacity: CFIndex,
    array: CFArrayRef,
) -> CFMutableArrayRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    let count = CFArrayGetCount(env, array);
    assert!(capacity == 0 || capacity >= count); // TODO: fixed capacity support
                                                 // The copy keeps the original's callbacks, so values that aren't objects
                                                 // (an array with NULL callbacks) are copied without being retained.
    let non_retaining: Class = msg_class![env; _touchHLE_NSArray_non_retaining class];
    let mutable_non_retaining: Class =
        msg_class![env; _touchHLE_NSMutableArray_non_retaining class];
    let is_non_retaining = msg![env; array isKindOfClass:non_retaining]
        || msg![env; array isKindOfClass:mutable_non_retaining];
    if !is_non_retaining {
        return msg![env; array mutableCopy];
    }
    let copy: id = msg_class![env; _touchHLE_NSMutableArray_non_retaining new];
    for i in 0..count {
        let value = CFArrayGetValueAtIndex(env, array, i);
        CFArrayAppendValue(env, copy, value);
    }
    copy
}

fn CFArrayGetCount(env: &mut Environment, array: CFArrayRef) -> CFIndex {
    let count: NSUInteger = msg![env; array count];
    count.try_into().unwrap()
}

fn CFArrayGetValueAtIndex(env: &mut Environment, array: CFArrayRef, idx: CFIndex) -> ConstVoidPtr {
    let idx: NSUInteger = idx.try_into().unwrap();
    let value: id = msg![env; array objectAtIndex:idx];
    value.cast().cast_const()
}

fn CFArrayAppendValue(env: &mut Environment, array: CFMutableArrayRef, value: ConstVoidPtr) {
    let value: id = value.cast().cast_mut();
    msg![env; array addObject:value]
}

fn CFArrayRemoveValueAtIndex(env: &mut Environment, array: CFMutableArrayRef, idx: CFIndex) {
    let idx: NSUInteger = idx.try_into().unwrap();
    msg![env; array removeObjectAtIndex:idx]
}

/// Sorts the values in the range with a guest comparison function, keeping
/// equal values in order.
fn CFArraySortValues(
    env: &mut Environment,
    array: CFMutableArrayRef,
    range: CFRange,
    comparator: GuestFunction, // CFComparatorFunction
    context: MutVoidPtr,
) {
    let mut values: Vec<ConstVoidPtr> = (range.location..(range.location + range.length))
        .map(|i| CFArrayGetValueAtIndex(env, array, i))
        .collect();
    // Insertion sort, as it's stable and the comparator is guest code.
    for i in 1..values.len() {
        let mut j = i;
        while j > 0 {
            let res: i32 = comparator.call_from_host(env, (values[j - 1], values[j], context));
            if res <= 0 {
                break;
            }
            values.swap(j - 1, j);
            j -= 1;
        }
    }
    for (i, value) in values.into_iter().enumerate() {
        let index: NSUInteger = (range.location + CFIndex::try_from(i).unwrap())
            .try_into()
            .unwrap();
        let value: id = value.cast().cast_mut();
        () = msg![env; array replaceObjectAtIndex:index withObject:value];
    }
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CFArrayCreateMutable(_, _, _)),
    export_c_func!(CFArrayCreateMutableCopy(_, _, _)),
    export_c_func!(CFArraySortValues(_, _, _, _)),
    export_c_func!(CFArrayGetCount(_)),
    export_c_func!(CFArrayGetValueAtIndex(_, _)),
    export_c_func!(CFArrayAppendValue(_, _)),
    export_c_func!(CFArrayRemoveValueAtIndex(_, _)),
];
