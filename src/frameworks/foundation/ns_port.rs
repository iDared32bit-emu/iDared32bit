/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSPort` and `NSMachPort`.
//!
//! Games mostly add a port to a thread's run loop just to keep it running.
//! Sending messages through ports isn't implemented.

use crate::objc::{autorelease, id, msg, nil, objc_classes, ClassExports, HostObject, NSZonePtr};

struct NSPortHostObject {
    /// Weak reference.
    delegate: id,
}
impl HostObject for NSPortHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSPort: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    // NSPort is abstract: like on iOS, make an NSMachPort instead.
    let class = if this == env.objc.get_known_class("NSPort", &mut env.mem) {
        env.objc.get_known_class("NSMachPort", &mut env.mem)
    } else {
        this
    };
    let host_object = Box::new(NSPortHostObject { delegate: nil });
    env.objc.alloc_object(class, host_object, &mut env.mem)
}

+ (id)port {
    let port: id = msg![env; this new];
    autorelease(env, port)
}

- (id)delegate {
    env.objc.borrow::<NSPortHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<NSPortHostObject>(this).delegate = delegate;
}

@end

@implementation NSMachPort: NSPort
@end

};
