/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSTimeZone`.

use crate::frameworks::foundation::{ns_string, NSInteger};
use crate::objc::{autorelease, id, nil, release, retain, ClassExports, HostObject, NSZonePtr};
use crate::{msg, objc_classes};

#[derive(Default)]
pub struct State {
    system_time_zone: Option<id>,
}

struct NSTimeZoneHostObject {
    // NSString*
    time_zone: id,
    /// TODO: only `timeZoneForSecondsFromGMT:` sets this; named zones are GMT.
    seconds_from_gmt: NSInteger,
}
impl HostObject for NSTimeZoneHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSTimeZone: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSTimeZoneHostObject {
        time_zone: nil,
        seconds_from_gmt: 0,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)timeZoneWithName:(id)tz_name {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithName:tz_name];
    autorelease(env, new)
}

+ (id)timeZoneForSecondsFromGMT:(NSInteger)seconds {
    // Named like iOS's, e.g. "GMT+0130".
    let minutes = seconds.abs() / 60;
    let name = if minutes == 0 {
        "GMT".to_string()
    } else {
        let sign = if seconds < 0 { '-' } else { '+' };
        format!("GMT{}{:02}{:02}", sign, minutes / 60, minutes % 60)
    };
    let name = ns_string::from_rust_string(env, name);
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithName:name];
    release(env, name);
    env.objc.borrow_mut::<NSTimeZoneHostObject>(new).seconds_from_gmt = seconds;
    autorelease(env, new)
}

+ (id)localTimeZone {
    // According to docs, `localTimeZone` is not cached in contrast to
    // `systemTimeZone`
    let gmt_tz_name: id = ns_string::get_static_str(env, "GMT");
    msg![env; this timeZoneWithName:gmt_tz_name]
}

+ (id)systemTimeZone {
    if let Some(system_time_zone) = env.framework_state.foundation.ns_time_zone.system_time_zone {
        system_time_zone
    } else {
        let new: id = msg![env; this alloc];
        let gmt_tz_name: id = ns_string::get_static_str(env, "GMT");
        let new: id = msg![env; new initWithName:gmt_tz_name];
        env.framework_state.foundation.ns_time_zone.system_time_zone = Some(new);
        new
    }
}

+ (id)defaultTimeZone {
    // TODO: implement setting a default time zone
    msg![env; this systemTimeZone]
}

- (())dealloc {
    let tz_name = env.objc.borrow_mut::<NSTimeZoneHostObject>(this).time_zone;
    release(env, tz_name);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)initWithName:(id)tz_name { // NSString *
    assert_ne!(tz_name, nil);
    retain(env, tz_name);
    env.objc.borrow_mut::<NSTimeZoneHostObject>(this).time_zone = tz_name;
    this
}

- (id)name {
    env.objc.borrow_mut::<NSTimeZoneHostObject>(this).time_zone
}

- (id)abbreviation {
    // TODO: support zone abbreviations
    ns_string::get_static_str(env, "GMT")
}

- (NSInteger)secondsFromGMT {
    env.objc.borrow::<NSTimeZoneHostObject>(this).seconds_from_gmt
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

@end

};
