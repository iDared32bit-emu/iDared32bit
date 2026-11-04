/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSSortDescriptor`.

use super::NSComparisonResult;
use crate::objc::{
    autorelease, id, msg, msg_class, msg_send, nil, objc_classes, release, ClassExports,
    HostObject, NSZonePtr, SEL,
};

struct NSSortDescriptorHostObject {
    /// `NSString*`, or nil to compare the objects themselves.
    key: id,
    ascending: bool,
    /// Compares the values, `compare:` unless another is given.
    selector: SEL,
}
impl HostObject for NSSortDescriptorHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSSortDescriptor: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let selector = env.objc.lookup_selector("compare:").unwrap();
    let host_object = Box::new(NSSortDescriptorHostObject {
        key: nil,
        ascending: true,
        selector,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)sortDescriptorWithKey:(id)key // NSString*
                  ascending:(bool)ascending {
    let new: id = msg_class![env; NSSortDescriptor alloc];
    let new: id = msg![env; new initWithKey:key ascending:ascending];
    autorelease(env, new)
}

+ (id)sortDescriptorWithKey:(id)key // NSString*
                  ascending:(bool)ascending
                   selector:(SEL)selector {
    let new: id = msg_class![env; NSSortDescriptor alloc];
    let new: id = msg![env; new initWithKey:key ascending:ascending selector:selector];
    autorelease(env, new)
}

- (id)initWithKey:(id)key // NSString*
        ascending:(bool)ascending {
    let selector = env.objc.lookup_selector("compare:").unwrap();
    msg![env; this initWithKey:key ascending:ascending selector:selector]
}

- (id)initWithKey:(id)key // NSString*
        ascending:(bool)ascending
         selector:(SEL)selector {
    let key: id = msg![env; key copy];
    *env.objc.borrow_mut(this) = NSSortDescriptorHostObject {
        key,
        ascending,
        selector,
    };
    this
}

- (id)key {
    env.objc.borrow::<NSSortDescriptorHostObject>(this).key
}

- (bool)ascending {
    env.objc.borrow::<NSSortDescriptorHostObject>(this).ascending
}

- (SEL)selector {
    env.objc.borrow::<NSSortDescriptorHostObject>(this).selector
}

- (NSComparisonResult)compareObject:(id)a toObject:(id)b {
    let &NSSortDescriptorHostObject { key, ascending, selector } = env.objc.borrow(this);
    let (a, b): (id, id) = if key == nil {
        (a, b)
    } else {
        (msg![env; a valueForKey:key], msg![env; b valueForKey:key])
    };
    let res: NSComparisonResult = msg_send(env, (a, selector, b));
    if ascending { res } else { -res }
}

- (())dealloc {
    let key = env.objc.borrow::<NSSortDescriptorHostObject>(this).key;
    release(env, key);
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};
