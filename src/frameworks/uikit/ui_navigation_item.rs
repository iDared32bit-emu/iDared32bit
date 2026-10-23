/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UINavigationItem`.

use crate::frameworks::foundation::ns_string::get_static_str;
use crate::objc::{
    id, msg, msg_super, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;

#[derive(Default)]
pub struct UINavigationItemHostObject {
    pub title: id,
    pub left_bar_button_item: id,
    pub right_bar_button_item: id,
    pub back_bar_button_item: id,
    pub title_view: id,
    pub hides_back_button: bool,
    /// The bar showing this item, if any (weak).
    pub navigation_bar: id,
}
impl HostObject for UINavigationItemHostObject {}

/// Replaces one of the item's retained fields, and lays out its bar again.
fn set_field(
    env: &mut Environment,
    this: id,
    value: id,
    field: fn(&mut UINavigationItemHostObject) -> &mut id,
) {
    retain(env, value);
    let host = env.objc.borrow_mut::<UINavigationItemHostObject>(this);
    let old = std::mem::replace(field(host), value);
    let bar = host.navigation_bar;
    release(env, old);
    if bar != nil {
        () = msg![env; bar setNeedsLayout];
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UINavigationItem: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UINavigationItemHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title { // NSString*
    let this: id = msg_super![env; this init];
    () = msg![env; this setTitle:title];
    this
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this init];
    let key = get_static_str(env, "UITitle");
    let title: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setTitle:title];
    let key = get_static_str(env, "UILeftBarButtonItem");
    let item: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setLeftBarButtonItem:item];
    let key = get_static_str(env, "UIRightBarButtonItem");
    let item: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setRightBarButtonItem:item];
    let key = get_static_str(env, "UIBackBarButtonItem");
    let item: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setBackBarButtonItem:item];
    let key = get_static_str(env, "UIHidesBackButton");
    let hides: bool = msg![env; coder decodeBoolForKey:key];
    () = msg![env; this setHidesBackButton:hides];
    this
}

- (())dealloc {
    let UINavigationItemHostObject {
        title,
        left_bar_button_item,
        right_bar_button_item,
        back_bar_button_item,
        title_view,
        ..
    } = std::mem::take(env.objc.borrow_mut(this));
    for object in [
        title,
        left_bar_button_item,
        right_bar_button_item,
        back_bar_button_item,
        title_view,
    ] {
        release(env, object);
    }
    msg_super![env; this dealloc]
}

- (id)title {
    env.objc.borrow::<UINavigationItemHostObject>(this).title
}
- (())setTitle:(id)title { // NSString*
    let title: id = msg![env; title copy];
    set_field(env, this, title, |host| &mut host.title);
    release(env, title);
}

- (id)leftBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_bar_button_item
}
- (())setLeftBarButtonItem:(id)item { // UIBarButtonItem*
    set_field(env, this, item, |host| &mut host.left_bar_button_item);
}
- (())setLeftBarButtonItem:(id)item animated:(bool)_animated {
    () = msg![env; this setLeftBarButtonItem:item];
}

- (id)rightBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).right_bar_button_item
}
- (())setRightBarButtonItem:(id)item { // UIBarButtonItem*
    set_field(env, this, item, |host| &mut host.right_bar_button_item);
}
- (())setRightBarButtonItem:(id)item animated:(bool)_animated {
    () = msg![env; this setRightBarButtonItem:item];
}

- (id)backBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).back_bar_button_item
}
- (())setBackBarButtonItem:(id)item { // UIBarButtonItem*
    set_field(env, this, item, |host| &mut host.back_bar_button_item);
}

- (id)titleView {
    env.objc.borrow::<UINavigationItemHostObject>(this).title_view
}
- (())setTitleView:(id)view { // UIView*
    set_field(env, this, view, |host| &mut host.title_view);
}

- (bool)hidesBackButton {
    env.objc.borrow::<UINavigationItemHostObject>(this).hides_back_button
}
- (())setHidesBackButton:(bool)hides {
    let host = env.objc.borrow_mut::<UINavigationItemHostObject>(this);
    host.hides_back_button = hides;
    let bar = host.navigation_bar;
    if bar != nil {
        () = msg![env; bar setNeedsLayout];
    }
}
- (())setHidesBackButton:(bool)hides animated:(bool)_animated {
    () = msg![env; this setHidesBackButton:hides];
}

@end

};
