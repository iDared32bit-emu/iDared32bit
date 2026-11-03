/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSURLConnection`.
//!
//! HTTP isn't implemented yet, so connections fail as on a device with no
//! network (e.g. in airplane mode): "not connected to the internet".

use super::ns_error::NSLocalizedDescriptionKey;
use super::ns_run_loop::add_perform_request;
use super::{ns_string, NSInteger};
use crate::environment::Environment;
use crate::mem::MutPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use std::borrow::Cow;

struct NSURLConnectionHostObject {
    /// `NSURLRequest*`
    request: id,
    /// Retained until the connection finishes or is cancelled, as on iOS.
    delegate: id,
    /// `NSRunLoop*` from `scheduleInRunLoop:forMode:`, else the one current
    /// when it starts.
    run_loop: id,
    started: bool,
    finished: bool,
}
impl HostObject for NSURLConnectionHostObject {}

const NSURLErrorDomain: &str = "NSURLErrorDomain";

/// Our helper type, Foundation just uses ints.
type NSURLErrorCode = NSInteger;
const NSURLErrorNotConnectedToInternet: NSURLErrorCode = -1009;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSURLConnection: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSURLConnectionHostObject {
        request: nil,
        delegate: nil,
        run_loop: nil,
        started: false,
        finished: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)sendSynchronousRequest:(id)request // NSURLRequest *
           returningResponse:(MutPtr<id>)response // NSURLResponse **
                       error:(MutPtr<id>)out_error { // NSError **
    log!(
        "TODO: [NSURLConnection sendSynchronousRequest:{:?} ('{}') response:{:?} error:{:?}] -> nil",
        request,
        url_string_from_request(env, request),
        response,
        out_error,
    );
    if !response.is_null() {
        env.mem.write(response, nil);
    }
    if !out_error.is_null() {
        let domain = ns_string::get_static_str(env, NSURLErrorDomain);
        let error = msg_class![env; NSError alloc];
        // TODO: fill userInfo
        let error = msg![env; error initWithDomain:domain code:NSURLErrorNotConnectedToInternet userInfo:nil];
        autorelease(env, error);
        env.mem.write(out_error, error);
    }
    nil
}

+ (id)connectionWithRequest:(id)request // NSURLRequest *
                   delegate:(id)delegate {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithRequest:request delegate:delegate];
    autorelease(env, new)
}

- (id)initWithRequest:(id)request // NSURLRequest *
             delegate:(id)delegate {
    msg![env; this initWithRequest:request delegate:delegate startImmediately:true]
}

- (id)initWithRequest:(id)request // NSURLRequest *
             delegate:(id)delegate
     startImmediately:(bool)start_immediately {
    log!(
        "[(NSURLConnection *){:?} initWithRequest:{:?} ('{}') delegate:{:?} startImmediately:{}]: {}",
        this,
        request,
        url_string_from_request(env, request),
        delegate,
        start_immediately,
        if env.options.network_access {
            "TODO: HTTP isn't implemented yet, so this fails as if offline"
        } else {
            "network access is disabled, so this fails as if offline"
        },
    );
    let request: id = msg![env; request copy];
    retain(env, delegate);
    let host_object = env.objc.borrow_mut::<NSURLConnectionHostObject>(this);
    host_object.request = request;
    host_object.delegate = delegate;
    if start_immediately {
        () = msg![env; this start];
    }
    this
}

- (())scheduleInRunLoop:(id)run_loop // NSRunLoop *
                forMode:(id)_mode { // NSString *
    // TODO: modes
    env.objc.borrow_mut::<NSURLConnectionHostObject>(this).run_loop = run_loop;
}

- (())start {
    let host_object = env.objc.borrow::<NSURLConnectionHostObject>(this);
    if host_object.started || host_object.finished {
        return;
    }
    let run_loop = if host_object.run_loop == nil {
        msg_class![env; NSRunLoop currentRunLoop]
    } else {
        host_object.run_loop
    };
    env.objc.borrow_mut::<NSURLConnectionHostObject>(this).started = true;
    // As on iOS, the delegate hears of it later, from the run loop.
    let sel = env.objc.lookup_selector("_touchHLE_failOffline").unwrap();
    add_perform_request(env, run_loop, this, sel, nil, None, false);
}

- (())cancel {
    let host_object = env.objc.borrow_mut::<NSURLConnectionHostObject>(this);
    if host_object.finished {
        return;
    }
    host_object.finished = true;
    let delegate = std::mem::replace(&mut host_object.delegate, nil);
    release(env, delegate);
}

- (())_touchHLE_failOffline {
    let host_object = env.objc.borrow_mut::<NSURLConnectionHostObject>(this);
    if host_object.finished {
        return;
    }
    host_object.finished = true;
    let delegate = std::mem::replace(&mut host_object.delegate, nil);

    let domain = ns_string::get_static_str(env, NSURLErrorDomain);
    let key = ns_string::get_static_str(env, NSLocalizedDescriptionKey);
    let description =
        ns_string::get_static_str(env, "The Internet connection appears to be offline.");
    let user_info: id = msg_class![env; NSDictionary dictionaryWithObject:description forKey:key];
    let error: id = msg_class![env; NSError errorWithDomain:domain
                                                       code:NSURLErrorNotConnectedToInternet
                                                   userInfo:user_info];
    let sel = env.objc.lookup_selector("connection:didFailWithError:");
    if sel.is_some_and(|sel| env.objc.object_has_method(&env.mem, delegate, sel)) {
        () = msg![env; delegate connection:this didFailWithError:error];
    }
    release(env, delegate);
}

- (id)originalRequest {
    env.objc.borrow::<NSURLConnectionHostObject>(this).request
}

- (())dealloc {
    let &NSURLConnectionHostObject { request, delegate, .. } = env.objc.borrow(this);
    release(env, request);
    release(env, delegate);
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

fn url_string_from_request(env: &mut Environment, request: id) -> Cow<'static, str> {
    if request == nil {
        Cow::from("(null)")
    } else {
        let url = msg![env; request URL];
        let ns_string = msg![env; url absoluteString];
        ns_string::to_rust_string(env, ns_string)
    }
}
