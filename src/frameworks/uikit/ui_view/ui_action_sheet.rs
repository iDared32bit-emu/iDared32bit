/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIActionSheet`.
//!
//! Like `UIAlertView`, the sheet is shown with the host's own alert (an SDL
//! messagebox): its buttons work, but it doesn't slide up from the bottom.

use crate::frameworks::core_graphics::CGRect;
use crate::frameworks::foundation::{ns_string, NSInteger};
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr, SEL,
};
use crate::Environment;

type UIActionSheetStyle = NSInteger;

#[derive(Default)]
pub struct UIActionSheetHostObject {
    superclass: super::UIViewHostObject,
    /// `NSString*`
    title: id,
    /// Weak reference, like in UIKit.
    delegate: id,
    /// `NSString*`s, in index order.
    button_titles: Vec<id>,
    cancel_button_index: NSInteger,
    destructive_button_index: NSInteger,
    style: UIActionSheetStyle,
    visible: bool,
}
impl_HostObject_with_superclass!(UIActionSheetHostObject);

fn add_button(env: &mut Environment, this: id, title: id) -> NSInteger {
    let title: id = msg![env; title copy];
    let host = env.objc.borrow_mut::<UIActionSheetHostObject>(this);
    host.button_titles.push(title);
    (host.button_titles.len() - 1) as NSInteger
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIActionSheet: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIActionSheetHostObject {
        cancel_button_index: -1,
        destructive_button_index: -1,
        ..Default::default()
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// As on iOS, the destructive button comes first and the cancel button last.
- (id)initWithTitle:(id)title
                     delegate:(id)delegate
            cancelButtonTitle:(id)cancelButtonTitle
       destructiveButtonTitle:(id)destructiveButtonTitle
            otherButtonTitles:(id)otherButtonTitles, ...args {
    let this: id = msg_super![env; this init];
    () = msg![env; this setTitle:title];
    () = msg![env; this setDelegate:delegate];
    if destructiveButtonTitle != nil {
        let index = add_button(env, this, destructiveButtonTitle);
        let host = env.objc.borrow_mut::<UIActionSheetHostObject>(this);
        host.destructive_button_index = index;
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
    if cancelButtonTitle != nil {
        let index = add_button(env, this, cancelButtonTitle);
        env.objc.borrow_mut::<UIActionSheetHostObject>(this).cancel_button_index = index;
    }
    this
}

- (())dealloc {
    let UIActionSheetHostObject {
        title,
        button_titles,
        ..
    } = std::mem::take(env.objc.borrow_mut(this));
    release(env, title);
    for title in button_titles {
        release(env, title);
    }
    msg_super![env; this dealloc]
}

- (id)title {
    env.objc.borrow::<UIActionSheetHostObject>(this).title
}
- (())setTitle:(id)title { // NSString*
    let title: id = msg![env; title copy];
    let host = env.objc.borrow_mut::<UIActionSheetHostObject>(this);
    let old = std::mem::replace(&mut host.title, title);
    release(env, old);
}

- (id)delegate {
    env.objc.borrow::<UIActionSheetHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).delegate = delegate;
}

// Only the look, which the host's alert doesn't have.
- (UIActionSheetStyle)actionSheetStyle {
    env.objc.borrow::<UIActionSheetHostObject>(this).style
}
- (())setActionSheetStyle:(UIActionSheetStyle)style {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).style = style;
}

- (bool)isVisible {
    env.objc.borrow::<UIActionSheetHostObject>(this).visible
}

- (NSInteger)addButtonWithTitle:(id)title { // NSString*
    add_button(env, this, title)
}

- (NSInteger)numberOfButtons {
    env.objc.borrow::<UIActionSheetHostObject>(this).button_titles.len() as NSInteger
}

- (id)buttonTitleAtIndex:(NSInteger)index {
    let titles = &env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    usize::try_from(index).ok().and_then(|i| titles.get(i).copied()).unwrap_or(nil)
}

- (NSInteger)cancelButtonIndex {
    env.objc.borrow::<UIActionSheetHostObject>(this).cancel_button_index
}
- (())setCancelButtonIndex:(NSInteger)index {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).cancel_button_index = index;
}

- (NSInteger)destructiveButtonIndex {
    env.objc.borrow::<UIActionSheetHostObject>(this).destructive_button_index
}
- (())setDestructiveButtonIndex:(NSInteger)index {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).destructive_button_index = index;
}

// The first button that is neither cancel nor destructive, or -1.
- (NSInteger)firstOtherButtonIndex {
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    (0..host.button_titles.len() as NSInteger)
        .find(|&i| i != host.cancel_button_index && i != host.destructive_button_index)
        .unwrap_or(-1)
}

// Where the sheet would appear doesn't matter for the host's alert.
- (())showInView:(id)_view { // UIView*
    show(env, this);
}
- (())showFromToolbar:(id)_toolbar { // UIToolbar*
    show(env, this);
}
- (())showFromTabBar:(id)_tab_bar { // UITabBar*
    show(env, this);
}
- (())showFromBarButtonItem:(id)_item // UIBarButtonItem*
                   animated:(bool)_animated {
    show(env, this);
}
- (())showFromRect:(CGRect)_rect
            inView:(id)_view // UIView*
          animated:(bool)_animated {
    show(env, this);
}

// The game closing it itself: no button was tapped.
- (())dismissWithClickedButtonIndex:(NSInteger)index
                           animated:(bool)_animated {
    if !env.objc.borrow::<UIActionSheetHostObject>(this).visible {
        return;
    }
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).visible = false;
    tell_delegate(env, this, "actionSheet:willDismissWithButtonIndex:", Some(index));
    tell_delegate(env, this, "actionSheet:didDismissWithButtonIndex:", Some(index));
}

- (())_touchHLE_clickedButtonAtIndex:(id)number { // NSNumber*
    let index: NSInteger = msg![env; number integerValue];
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).visible = false;
    for name in [
        "actionSheet:clickedButtonAtIndex:",
        "actionSheet:willDismissWithButtonIndex:",
        "actionSheet:didDismissWithButtonIndex:",
    ] {
        tell_delegate(env, this, name, Some(index));
    }
    release(env, this);
}

@end

};

/// Sends `name` to the delegate, if it has it, with the sheet and, if
/// given, a button index.
fn tell_delegate(env: &mut Environment, this: id, name: &str, index: Option<NSInteger>) {
    let delegate = env.objc.borrow::<UIActionSheetHostObject>(this).delegate;
    if delegate == nil {
        return;
    }
    let sel: SEL = env
        .objc
        .register_host_selector(name.to_string(), &mut env.mem);
    let responds: bool = msg![env; delegate respondsToSelector:sel];
    if !responds {
        return;
    }
    match index {
        Some(index) => () = crate::objc::msg_send(env, (delegate, sel, this, index)),
        None => () = crate::objc::msg_send(env, (delegate, sel, this)),
    }
}

/// Shows the sheet with the host's alert. On a device, showing returns at
/// once and the delegate hears about the tap later, from the run loop.
fn show(env: &mut Environment, this: id) {
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    let title = host.title;
    let (button_titles, cancel_index) = (host.button_titles.clone(), host.cancel_button_index);
    let to_string = |env: &mut Environment, s: id| {
        if s == nil {
            String::new()
        } else {
            ns_string::to_rust_string(env, s).into_owned()
        }
    };
    let title = to_string(env, title);
    let buttons: Vec<String> = button_titles.iter().map(|&b| to_string(env, b)).collect();
    log!("UIActionSheet: title: {:?}, buttons: {:?}", title, buttons);

    env.objc.borrow_mut::<UIActionSheetHostObject>(this).visible = true;
    tell_delegate(env, this, "willPresentActionSheet:", None);
    tell_delegate(env, this, "didPresentActionSheet:", None);

    let cancel_button = usize::try_from(cancel_index)
        .ok()
        .filter(|&i| i < buttons.len());
    let index = env.on_parent_stack_in_coroutine(|window, _| {
        window.show_alert(&title, "", &buttons, cancel_button)
    });
    // Closing it without a button counts as the cancel button, as tapping
    // outside an iPad's sheet does.
    let Some(index) = index.or(cancel_button) else {
        env.objc.borrow_mut::<UIActionSheetHostObject>(this).visible = false;
        return;
    };
    log!(
        "UIActionSheet: button {} ({:?}) tapped",
        index,
        buttons[index]
    );

    retain(env, this);
    let number: id = msg_class![env; NSNumber numberWithInteger:(index as NSInteger)];
    let sel = env
        .objc
        .register_host_selector("_touchHLE_clickedButtonAtIndex:".to_string(), &mut env.mem);
    () = msg![env; this performSelector:sel withObject:number afterDelay:0.0];
}
