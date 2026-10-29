/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CFDateFormatter`

use super::cf_allocator::{kCFAllocatorDefault, CFAllocatorRef};
use super::cf_locale::CFLocaleRef;
use super::cf_string::CFStringRef;
use super::time::CFAbsoluteTime;
use super::{CFIndex, CFTypeRef};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::foundation::NSUInteger;
use crate::objc::{id, msg, msg_class, retain};
use crate::Environment;

/// `NSDateFormatter` is toll-free bridged with `CFDateFormatterRef`
type CFDateFormatterRef = CFTypeRef;
type CFDateFormatterStyle = CFIndex;
type CFDateRef = CFTypeRef;

fn CFDateFormatterCreate(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    _locale: CFLocaleRef, // NSDateFormatter only formats as en_US
    date_style: CFDateFormatterStyle,
    time_style: CFDateFormatterStyle,
) -> CFDateFormatterRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    let formatter: id = msg_class![env; NSDateFormatter new];
    let date_style = date_style as NSUInteger;
    let time_style = time_style as NSUInteger;
    () = msg![env; formatter setDateStyle:date_style];
    () = msg![env; formatter setTimeStyle:time_style];
    formatter
}

fn CFDateFormatterSetFormat(
    env: &mut Environment,
    formatter: CFDateFormatterRef,
    format: CFStringRef,
) {
    () = msg![env; formatter setDateFormat:format];
}

fn CFDateFormatterCreateStringWithDate(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    formatter: CFDateFormatterRef,
    date: CFDateRef,
) -> CFStringRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    let string: id = msg![env; formatter stringFromDate:date];
    retain(env, string)
}

fn CFDateFormatterCreateStringWithAbsoluteTime(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    formatter: CFDateFormatterRef,
    at: CFAbsoluteTime,
) -> CFStringRef {
    let date: id = msg_class![env; NSDate dateWithTimeIntervalSinceReferenceDate:at];
    CFDateFormatterCreateStringWithDate(env, allocator, formatter, date)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CFDateFormatterCreate(_, _, _, _)),
    export_c_func!(CFDateFormatterSetFormat(_, _)),
    export_c_func!(CFDateFormatterCreateStringWithDate(_, _, _)),
    export_c_func!(CFDateFormatterCreateStringWithAbsoluteTime(_, _, _)),
];
