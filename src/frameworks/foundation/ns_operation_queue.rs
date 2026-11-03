/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSOperationQueue`.
//!
//! Resources:
//! - [Apple's Concurrency Programming Guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ConcurrencyProgrammingGuide/OperationObjects/OperationObjects.html)

use std::time::Duration;

use super::{ns_array, NSInteger, NSUInteger};
use crate::abi::GuestFunction;
use crate::dyld::HostFunction;
use crate::environment::Environment;
use crate::mem::MutVoidPtr;
use crate::msg_class;
use crate::objc::{
    autorelease, id, msg, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr,
};

pub struct State {
    helper_guest_func: GuestFunction,
}
impl State {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env.framework_state.foundation.ns_operation_queue
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            helper_guest_func: GuestFunction::null_ptr(),
        }
    }
}

/// `NSOperationQueueDefaultMaxConcurrentOperationCount`: no limit.
const DEFAULT_MAX_CONCURRENT: NSInteger = -1;

struct NSOperationQueueHostObject {
    /// In the order they were added, retained until they've finished.
    operations: Vec<id>,
    /// The operations that have been started.
    started: Vec<id>,
    max_concurrent: NSInteger,
    /// Whether the queue's thread is running. It exits when the queue is
    /// empty, and is started again when an operation is added.
    thread_running: bool,
}
impl HostObject for NSOperationQueueHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);
@implementation NSOperationQueue: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSOperationQueueHostObject {
        operations: Vec::new(),
        started: Vec::new(),
        max_concurrent: DEFAULT_MAX_CONCURRENT,
        thread_running: false,
    };
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

- (())addOperation:(id)operation {
    let operation = retain(env, operation);
    let host_object = env.objc.borrow_mut::<NSOperationQueueHostObject>(this);
    host_object.operations.push(operation);
    if host_object.thread_running {
        return;
    }
    host_object.thread_running = true;
    let state = State::get(env);
    let operation_queue_gf = if state.helper_guest_func.to_ptr().is_null() {
        let operation_queue_hf: HostFunction =
            &(operation_queue_thread_helper as fn(&mut Environment, _) -> _);
        let operation_queue_gf = env.dyld.create_guest_function(
            env.mem.as_mut(),
            "__touchHLE_operation_queue_helper",
            operation_queue_hf,
        );
        let state = State::get(env);
        state.helper_guest_func = operation_queue_gf;
        operation_queue_gf
    } else {
        state.helper_guest_func
    };
    // The thread keeps the queue alive until it's empty.
    retain(env, this);
    env.new_thread(
        operation_queue_gf,
        this.cast(),
        crate::mem::Mem::SECONDARY_THREAD_DEFAULT_STACK_SIZE,
    );
}

- (())cancelAllOperations {
    let operations = env.objc.borrow::<NSOperationQueueHostObject>(this).operations.clone();
    for operation in operations {
        let _: () = msg![env; operation cancel];
    }
}

// Those that have not finished yet, as on iOS.
- (id)operations {
    let operations = env.objc.borrow::<NSOperationQueueHostObject>(this).operations.clone();
    for &operation in &operations {
        retain(env, operation);
    }
    let array = ns_array::from_vec(env, operations);
    autorelease(env, array)
}
- (NSUInteger)operationCount {
    env.objc.borrow::<NSOperationQueueHostObject>(this).operations.len() as NSUInteger
}

- (NSInteger)maxConcurrentOperationCount {
    env.objc.borrow::<NSOperationQueueHostObject>(this).max_concurrent
}
- (())setMaxConcurrentOperationCount:(NSInteger)count {
    env.objc.borrow_mut::<NSOperationQueueHostObject>(this).max_concurrent = count;
}

- (())waitUntilAllOperationsAreFinished {
    // The queue's thread removes them once they've finished.
    while !env.objc.borrow::<NSOperationQueueHostObject>(this).operations.is_empty() {
        env.sleep(Duration::from_millis(1));
    }
}

- (())dealloc {
    // The thread exits, releasing the queue, only once it's empty.
    assert!(env.objc.borrow::<NSOperationQueueHostObject>(this).operations.is_empty());
    env.objc.dealloc_object(this, &mut env.mem)
}

@end
};

/// The queue's thread. Operations are started in the order they were added
/// once they're ready, up to the queue's limit. Non-concurrent ones run here,
/// so one at a time; concurrent ones run themselves, and this waits for them
/// to say they've finished.
fn operation_queue_thread_helper(env: &mut Environment, data: MutVoidPtr) -> MutVoidPtr {
    let this: id = data.cast();
    loop {
        let pool: id = msg_class![env; NSAutoreleasePool new];

        let operations = env
            .objc
            .borrow::<NSOperationQueueHostObject>(this)
            .operations
            .clone();
        for operation in operations {
            let finished: bool = msg![env; operation isFinished];
            if finished {
                let host_object = env.objc.borrow_mut::<NSOperationQueueHostObject>(this);
                host_object.operations.retain(|&op| op != operation);
                host_object.started.retain(|&op| op != operation);
                release(env, operation);
            }
        }
        let host_object = env.objc.borrow::<NSOperationQueueHostObject>(this);
        let (max, running) = (host_object.max_concurrent, host_object.started.len());
        let waiting: Vec<id> = host_object
            .operations
            .iter()
            .filter(|op| !host_object.started.contains(op))
            .copied()
            .collect();
        let mut next = None;
        if max < 0 || (running as NSInteger) < max {
            for operation in waiting {
                // Cancelled ones are started too, which finishes them.
                let cancelled: bool = msg![env; operation isCancelled];
                let ready: bool = msg![env; operation isReady];
                if cancelled || ready {
                    next = Some(operation);
                    break;
                }
            }
        }
        if let Some(operation) = next {
            env.objc
                .borrow_mut::<NSOperationQueueHostObject>(this)
                .started
                .push(operation);
            let _: () = msg![env; operation start];
        }
        let _: () = msg![env; pool drain];
        // Checked last, as draining can run code that adds an operation.
        let host_object = env.objc.borrow_mut::<NSOperationQueueHostObject>(this);
        if host_object.operations.is_empty() {
            host_object.thread_running = false;
            break;
        }
        if next.is_none() {
            env.sleep(Duration::from_millis(5));
        }
    }
    release(env, this);
    MutVoidPtr::null()
}
