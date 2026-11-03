/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSAssertionHandler`, which `NSAssert` and friends call when a game's
//! assertion fails.

use super::ns_string::{to_rust_string, with_format};
use super::NSInteger;
use crate::objc::{id, msg, nil, objc_classes, ClassExports, TrivialHostObject, SEL};
use crate::Environment;

#[derive(Default)]
pub struct State {
    handler: Option<id>,
}

/// The game's description of the failure, if it gave one.
fn description(env: &mut Environment, format: id, args: crate::abi::VaList) -> String {
    if format == nil {
        String::new()
    } else {
        with_format(env, format, args)
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSAssertionHandler: NSObject

// iOS has one per thread, but they have no state, so one does for all.
+ (id)currentHandler {
    if let Some(handler) = env.framework_state.foundation.ns_assertion_handler.handler {
        return handler;
    }
    let new = env.objc.alloc_static_object(this, Box::new(TrivialHostObject), &mut env.mem);
    env.framework_state.foundation.ns_assertion_handler.handler = Some(new);
    new
}

// On iOS these raise NSInternalInconsistencyException, which ends the game
// unless it catches it. Exceptions aren't supported, so they stop it here,
// with the same message.
- (())handleFailureInMethod:(SEL)selector
                     object:(id)object
                       file:(id)file_name // NSString*
                 lineNumber:(NSInteger)line
                description:(id)format, // NSString*
                            ...args {
    let class: id = msg![env; object class];
    let class_name = env.objc.get_class_name(class).to_owned();
    let is_class = class == object;
    let method = selector.as_str(&env.mem).to_owned();
    let file = to_rust_string(env, file_name);
    let desc = description(env, format, args.start());
    panic!(
        "*** Assertion failure in {}[{} {}], {}:{}: {}",
        if is_class { "+" } else { "-" },
        class_name,
        method,
        file,
        line,
        desc,
    );
}

- (())handleFailureInFunction:(id)function_name // NSString*
                         file:(id)file_name // NSString*
                   lineNumber:(NSInteger)line
                  description:(id)format, // NSString*
                              ...args {
    let function = to_rust_string(env, function_name);
    let file = to_rust_string(env, file_name);
    let desc = description(env, format, args.start());
    panic!("*** Assertion failure in {function}(), {file}:{line}: {desc}");
}

@end

};
