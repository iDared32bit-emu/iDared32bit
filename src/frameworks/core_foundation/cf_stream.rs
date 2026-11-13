/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CFReadStream`, toll-free bridged to `NSInputStream`. Only file streams
//! are supported, as for `NSInputStream`.

use super::cf_allocator::{kCFAllocatorDefault, CFAllocatorRef};
use super::cf_url::CFURLRef;
use super::{CFIndex, CFTypeRef};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::mem::MutPtr;
use crate::objc::{id, msg, msg_class};
use crate::Environment;

type CFReadStreamRef = CFTypeRef;

/// `NSStreamStatusOpen`.
const OPEN: NSUInteger = 2;

fn CFReadStreamCreateWithFile(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    file_url: CFURLRef,
) -> CFReadStreamRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    let path: id = msg![env; file_url path];
    let stream: id = msg_class![env; NSInputStream alloc];
    msg![env; stream initWithFileAtPath:path]
}

// CFNetwork's HTTP streams aren't made yet (`CFReadStreamCreateForHTTPRequest`
// returns NULL), so NULL streams are handled too.
fn CFReadStreamOpen(env: &mut Environment, stream: CFReadStreamRef) -> bool {
    if stream.is_null() {
        return false;
    }
    () = msg![env; stream open];
    let status: NSUInteger = msg![env; stream streamStatus];
    status == OPEN
}

fn CFReadStreamRead(
    env: &mut Environment,
    stream: CFReadStreamRef,
    buffer: MutPtr<u8>,
    buffer_length: CFIndex,
) -> CFIndex {
    if stream.is_null() {
        return -1; // Not open.
    }
    let length: NSUInteger = buffer_length.max(0).try_into().unwrap();
    let read: NSInteger = msg![env; stream read:(buffer.cast_void()) maxLength:length];
    read
}

fn CFReadStreamHasBytesAvailable(env: &mut Environment, stream: CFReadStreamRef) -> bool {
    msg![env; stream hasBytesAvailable]
}

fn CFReadStreamClose(env: &mut Environment, stream: CFReadStreamRef) {
    msg![env; stream close]
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CFReadStreamCreateWithFile(_, _)),
    export_c_func!(CFReadStreamOpen(_)),
    export_c_func!(CFReadStreamRead(_, _, _)),
    export_c_func!(CFReadStreamHasBytesAvailable(_)),
    export_c_func!(CFReadStreamClose(_)),
];
