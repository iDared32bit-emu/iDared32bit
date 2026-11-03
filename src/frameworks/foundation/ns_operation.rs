/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSOperation`, and associated classes
//!
//! Resources:
//! - [Apple's Concurrency Programming Guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ConcurrencyProgrammingGuide/OperationObjects/OperationObjects.html)

use super::ns_string::get_static_str;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_super, nil, objc_classes, release, retain,
    ClassExports, HostObject, NSZonePtr, SEL,
};
use crate::Environment;

struct NSOperationHostObject {
    is_canceled: bool,
    is_executing: bool,
    is_finished: bool,
    /// Operations that must finish first. Retained.
    dependencies: Vec<id>,
}
impl HostObject for NSOperationHostObject {}

impl NSOperationHostObject {
    pub fn new() -> Self {
        Self {
            is_canceled: false,
            is_executing: false,
            is_finished: false,
            dependencies: Vec::new(),
        }
    }
}

/// Changes a state, with the key-value observing notifications iOS sends,
/// which some apps watch for.
fn set_state(
    env: &mut Environment,
    this: id,
    key: &'static str,
    change: impl FnOnce(&mut NSOperationHostObject),
) {
    let key = get_static_str(env, key);
    () = msg![env; this willChangeValueForKey:key];
    change(env.objc.borrow_mut(this));
    () = msg![env; this didChangeValueForKey:key];
}

struct NSInvocationOperationHostObject {
    superclass: NSOperationHostObject,
    target: id,
    sel: Option<SEL>,
    arg: id,
}
impl_HostObject_with_superclass!(NSInvocationOperationHostObject);

impl NSInvocationOperationHostObject {
    pub fn new() -> Self {
        Self {
            superclass: NSOperationHostObject::new(),
            target: nil,
            sel: None,
            arg: nil,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);
@implementation NSOperation: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSOperationHostObject::new();
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

- (())start {
    let host_object: &NSOperationHostObject = env.objc.borrow(this);
    if host_object.is_executing || host_object.is_finished {
        // iOS raises an exception.
        log!("Warning: {:?} was started again, ignoring", this);
        return;
    }
    // A cancelled operation finishes without running.
    if !host_object.is_canceled {
        set_state(env, this, "isExecuting", |o| o.is_executing = true);
        let _: () = msg![env; this main];
        set_state(env, this, "isExecuting", |o| o.is_executing = false);
    }
    set_state(env, this, "isFinished", |o| o.is_finished = true);
}

- (())main {
    // "The default implementation of this method does nothing."
}

- (())cancel {
    if !env.objc.borrow::<NSOperationHostObject>(this).is_canceled {
        set_state(env, this, "isCancelled", |o| o.is_canceled = true);
    }
}

- (bool)isCancelled {
    env.objc.borrow::<NSOperationHostObject>(this).is_canceled
}
- (bool)isExecuting {
    env.objc.borrow::<NSOperationHostObject>(this).is_executing
}
- (bool)isFinished {
    env.objc.borrow::<NSOperationHostObject>(this).is_finished
}
- (bool)isConcurrent {
    false
}
// Ready once all its dependencies have finished.
- (bool)isReady {
    let dependencies = env.objc.borrow::<NSOperationHostObject>(this).dependencies.clone();
    dependencies.into_iter().all(|op| msg![env; op isFinished])
}

- (())addDependency:(id)operation { // NSOperation*
    retain(env, operation);
    env.objc.borrow_mut::<NSOperationHostObject>(this).dependencies.push(operation);
}
- (())removeDependency:(id)operation { // NSOperation*
    let dependencies = &mut env.objc.borrow_mut::<NSOperationHostObject>(this).dependencies;
    if let Some(i) = dependencies.iter().position(|&op| op == operation) {
        dependencies.remove(i);
        release(env, operation);
    }
}

- (())dealloc {
    let dependencies = std::mem::take(
        &mut env.objc.borrow_mut::<NSOperationHostObject>(this).dependencies,
    );
    for operation in dependencies {
        release(env, operation);
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

@implementation NSInvocationOperation: NSOperation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSInvocationOperationHostObject::new();
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

- (id)initWithTarget:(id)target selector:(SEL)sel object:(id)arg {
    let this: id  = msg_super![env; this init];
    let target = retain(env, target);
    let arg = retain(env, arg);
    if !env.objc.object_has_method(env.mem.as_mut(), target, sel) {
        return nil;
    }
    let host_object = env.objc.borrow_mut::<NSInvocationOperationHostObject>(this);
    host_object.target = target;
    host_object.sel = Some(sel);
    host_object.arg = arg;
    this
}

- (())main {
    let host_object = env.objc.borrow::<NSInvocationOperationHostObject>(this);

    let target = host_object.target;
    let sel = host_object.sel.unwrap();
    let arg = host_object.arg;
    log_dbg!("Running NSInvocationOperation [{:?} {:?}({}) {:?}]", target, sel, sel.as_str(env.mem.as_mut()),  arg);
    if arg.is_null() {
        let args = (target, sel);
        let _: () = crate::objc::msg_send(env, args);
    } else {
        let args = (target, sel, arg);
        let _: () = crate::objc::msg_send(env, args);
    }
}

-(())dealloc {
    let host_object = env.objc.borrow::<NSInvocationOperationHostObject>(this);
    let target = host_object.target;
    let arg = host_object.arg;
    release(env, target);
    release(env, arg);
    env.objc.dealloc_object(this, env.mem.as_mut());
}

@end

};
