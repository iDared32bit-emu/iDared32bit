/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIAlertView`.
//!
//! The alert is shown with the host's own alert (an SDL messagebox) rather
//! than drawn as a view.

use crate::frameworks::foundation::{ns_string, NSInteger};
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr, SEL,
};

#[derive(Default)]
pub struct UIAlertViewHostObject {
    superclass: super::UIViewHostObject,
    /// `NSString*`
    title: id,
    /// `NSString*`
    message: id,
    /// Weak reference, like in UIKit.
    delegate: id,
    /// `NSString*`s, in index order.
    button_titles: Vec<id>,
    cancel_button_index: NSInteger,
}
impl_HostObject_with_superclass!(UIAlertViewHostObject);

fn add_button(env: &mut crate::Environment, this: id, title: id) -> NSInteger {
    let title: id = msg![env; title copy];
    let host = env.objc.borrow_mut::<UIAlertViewHostObject>(this);
    host.button_titles.push(title);
    (host.button_titles.len() - 1) as NSInteger
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIAlertView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIAlertViewHostObject {
        cancel_button_index: -1,
        ..Default::default()
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title
                      message:(id)message
                     delegate:(id)delegate
            cancelButtonTitle:(id)cancelButtonTitle
            otherButtonTitles:(id)otherButtonTitles, ...args {
    let this: id = msg_super![env; this init];
    () = msg![env; this setTitle:title];
    () = msg![env; this setMessage:message];
    () = msg![env; this setDelegate:delegate];
    if cancelButtonTitle != nil {
        let index = add_button(env, this, cancelButtonTitle);
        env.objc.borrow_mut::<UIAlertViewHostObject>(this).cancel_button_index = index;
    }
    if otherButtonTitles != nil {
        add_button(env, this, otherButtonTitles);
        let mut varargs = args.start();
        loop {
            let title: id = varargs.next(env);
            if title == nil {
                break;
            }
            add_button(env, this, title);
        }
    }
    this
}

- (())dealloc {
    let UIAlertViewHostObject {
        title,
        message,
        button_titles,
        ..
    } = std::mem::take(env.objc.borrow_mut(this));
    release(env, title);
    release(env, message);
    for title in button_titles {
        release(env, title);
    }
    msg_super![env; this dealloc]
}

- (id)title {
    env.objc.borrow::<UIAlertViewHostObject>(this).title
}
- (())setTitle:(id)title { // NSString*
    let title: id = msg![env; title copy];
    let host = env.objc.borrow_mut::<UIAlertViewHostObject>(this);
    let old = std::mem::replace(&mut host.title, title);
    release(env, old);
}

- (id)message {
    env.objc.borrow::<UIAlertViewHostObject>(this).message
}
- (())setMessage:(id)message { // NSString*
    let message: id = msg![env; message copy];
    let host = env.objc.borrow_mut::<UIAlertViewHostObject>(this);
    let old = std::mem::replace(&mut host.message, message);
    release(env, old);
}

- (id)delegate {
    env.objc.borrow::<UIAlertViewHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).delegate = delegate;
}

- (NSInteger)addButtonWithTitle:(id)title { // NSString*
    add_button(env, this, title)
}

- (NSInteger)numberOfButtons {
    env.objc.borrow::<UIAlertViewHostObject>(this).button_titles.len() as NSInteger
}

- (id)buttonTitleAtIndex:(NSInteger)index {
    let titles = &env.objc.borrow::<UIAlertViewHostObject>(this).button_titles;
    usize::try_from(index).ok().and_then(|i| titles.get(i).copied()).unwrap_or(nil)
}

- (NSInteger)cancelButtonIndex {
    env.objc.borrow::<UIAlertViewHostObject>(this).cancel_button_index
}
// The first button that isn't the cancel button, or -1 if there's none.
- (NSInteger)firstOtherButtonIndex {
    let host = env.objc.borrow::<UIAlertViewHostObject>(this);
    (0..host.button_titles.len() as NSInteger)
        .find(|&i| i != host.cancel_button_index)
        .unwrap_or(-1)
}
- (())setCancelButtonIndex:(NSInteger)index {
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).cancel_button_index = index;
}

- (())show {
    let host = env.objc.borrow::<UIAlertViewHostObject>(this);
    let (title, message) = (host.title, host.message);
    let (button_titles, cancel_index) = (host.button_titles.clone(), host.cancel_button_index);
    let to_string = |env: &mut crate::Environment, s: id| {
        if s == nil {
            String::new()
        } else {
            ns_string::to_rust_string(env, s).into_owned()
        }
    };
    let title = to_string(env, title);
    let message = to_string(env, message);
    let buttons: Vec<String> = button_titles.iter().map(|&b| to_string(env, b)).collect();
    log!("UIAlertView: title: {:?}, message: {:?}, buttons: {:?}", title, message, buttons);

    let cancel_button = usize::try_from(cancel_index)
        .ok()
        .filter(|&i| i < buttons.len());
    let index = env.on_parent_stack_in_coroutine(|window, _| {
        window.show_alert(&title, &message, &buttons, cancel_button)
    });
    // Closing the alert without a button counts as the cancel button, like
    // UIKit does when the app is sent to the background.
    let Some(index) = index.or(cancel_button) else {
        return;
    };
    log!("UIAlertView: button {} ({:?}) tapped", index, buttons[index]);

    // On a device, show returns at once and the delegate hears about the tap
    // later, from the run loop.
    retain(env, this);
    let number: id = msg_class![env; NSNumber numberWithInteger:(index as NSInteger)];
    let sel = env
        .objc
        .register_host_selector("_touchHLE_clickedButtonAtIndex:".to_string(), &mut env.mem);
    () = msg![env; this performSelector:sel withObject:number afterDelay:0.0];
}

- (())_touchHLE_clickedButtonAtIndex:(id)number { // NSNumber*
    let index: NSInteger = msg![env; number integerValue];
    let delegate = env.objc.borrow::<UIAlertViewHostObject>(this).delegate;
    if delegate != nil {
        for name in [
            "alertView:clickedButtonAtIndex:",
            "alertView:willDismissWithButtonIndex:",
            "alertView:didDismissWithButtonIndex:",
        ] {
            let sel: SEL = env.objc.register_host_selector(name.to_string(), &mut env.mem);
            let responds: bool = msg![env; delegate respondsToSelector:sel];
            if responds {
                () = crate::objc::msg_send(env, (delegate, sel, this, index));
            }
        }
    }
    release(env, this);
}

@end

};
