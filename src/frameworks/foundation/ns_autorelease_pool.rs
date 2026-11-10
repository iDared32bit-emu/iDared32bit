/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSAutoreleasePool`.

use crate::objc::{id, msg, objc_classes, release, ClassExports, HostObject, NSZonePtr};
use crate::{Environment, ThreadId};
use std::num::NonZeroU32;

#[derive(Default)]
pub struct ThreadLocalState {
    pool_stack: Option<Vec<id>>,
}
impl ThreadLocalState {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env.get_tl_framework_state().foundation.ns_autorelease_pool
    }
}

struct NSAutoreleasePoolHostObject {
    original_thread: ThreadId,
    /// This is allowed to contain duplicates, which get released several times!
    /// Each has its serial number (see [crate::objc::ObjC::object_serial]).
    objects: Vec<(id, Option<u64>)>,
}
impl HostObject for NSAutoreleasePoolHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSAutoreleasePool: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSAutoreleasePoolHostObject {
        original_thread: env.current_thread,
        objects: Vec::new(),
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (())addObject:(id)obj {
    let current_thread = env.current_thread;
    if let Some(current_pool) = ThreadLocalState::get(env)
        .pool_stack
        .as_ref()
        .and_then(|pool_stack| pool_stack.last().copied())
    {
        msg![env; current_pool addObject:obj]
    } else {
        log_dbg!(
            "Warning: no active NSAutoreleasePool, leaking {:?}, current thread {}",
            obj,
            current_thread
        );
    }
}

- (id)init {
    let current_thread = env.current_thread;
    let pool_stack = ThreadLocalState::get(env).pool_stack.get_or_insert_default();
    pool_stack.push(this);
    log_dbg!("New pool: {:?}, current thread {}", this, current_thread);
    this
}

- (())addObject:(id)obj {
    let serial = env.objc.object_serial(obj);
    env.objc.borrow_mut::<NSAutoreleasePoolHostObject>(this).objects.push((obj, serial));
}

- (id)retain {
    // TODO: throw proper exception?
    panic!("NSAutoreleasePool can't be retained!");
}
- (id)autorelease {
    // TODO: throw proper exception?
    panic!("NSAutoreleasePool can't be autoreleased!");
}

- (())drain {
    msg![env; this release]
}

- (())dealloc {
    let current_thread = env.current_thread;
    log_dbg!(
        "Draining pool: {:?}, current thread {}",
        this,
        current_thread
    );
    let host_obj: &mut NSAutoreleasePoolHostObject = env.objc.borrow_mut(this);
    // It's unclear what should happen when draining a pool on the wrong thread,
    // but we prefer to be conservative here
    assert_eq!(host_obj.original_thread, current_thread);
    let pool_stack = ThreadLocalState::get(env).pool_stack.as_mut().unwrap();
    // NSAutoReleasePool seems to keep popping until reaches the appropriate
    // pool object. If there are pools that are "above" it in the stack, it
    // deallocates them as well.
    let Some((index, _)) = pool_stack
        .iter()
        .enumerate()
        .rev()
        .find(|(_, pool)| **pool == this)
    else {
        panic!(
            "Bad [{:?} (NSAutoReleasePool) release] on thread {}!",
            this, env.current_thread
        )
    };
    let to_drop: Vec<id> = pool_stack.drain(index..).collect();
    log_dbg!("Dropping pools {:?}", to_drop);
    for pool in to_drop.into_iter().rev() {
        if pool != this {
            // It's a bit ugly, but we cannot call a release on those other
            // pools as we already drained the shared pool stacks.
            // So we manually decrement and dealloc instead.
            // TODO: refactor this
            assert_eq!(env.objc.get_refcount(pool), NonZeroU32::new(1).unwrap());
            _ = env.objc.decrement_refcount(pool);
        }
        let host_obj: &mut NSAutoreleasePoolHostObject = env.objc.borrow_mut(pool);
        let objects = std::mem::take(&mut host_obj.objects);
        env.objc.dealloc_object(pool, &mut env.mem);
        for (object, serial) in objects {
            // A game that releases an object it doesn't own can free it while
            // it's still here. On iOS the freed object usually still answers
            // the release; here its address may hold another object by now.
            if serial.is_some() && env.objc.object_serial(object) != serial {
                log!("Warning: {:?} was freed before its autorelease pool drained (released too many times by the game), skipping.", object);
                continue;
            }
            release(env, object);
        }
    }
}

@end

};
