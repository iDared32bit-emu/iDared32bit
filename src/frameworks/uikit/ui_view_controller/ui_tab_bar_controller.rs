/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITabBarController`, `UITabBar` and `UITabBarItem`.
//!
//! TODO: this is a stub. The selected tab's view is shown above a plain
//! black bar, and tabs can be switched from code, but the bar's items aren't
//! drawn and tapping them does nothing. System items have no title or
//! image.

use super::UIViewControllerHostObject;
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{ns_array, NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_application::UIInterfaceOrientation;
use crate::frameworks::uikit::ui_bar_item::UIBarItemHostObject;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_send, msg_super, nil,
    objc_classes, release, retain, todo_objc_setter, ClassExports, NSZonePtr,
};
use crate::Environment;

const TAB_BAR_HEIGHT: CGFloat = 49.0;

#[derive(Default)]
struct UITabBarControllerHostObject {
    superclass: UIViewControllerHostObject,
    /// Retained.
    view_controllers: Vec<id>,
    selected_index: NSUInteger,
    /// Weak.
    delegate: id,
    /// `UITabBar*`, made when first asked for. Retained.
    tab_bar: id,
}
impl_HostObject_with_superclass!(UITabBarControllerHostObject);

#[derive(Default)]
struct UITabBarItemHostObject {
    superclass: UIBarItemHostObject,
    /// `UIImage*`, retained.
    image: id,
    /// `NSString*`, retained.
    badge_value: id,
}
impl_HostObject_with_superclass!(UITabBarItemHostObject);

fn selected(env: &Environment, this: id) -> id {
    let host_object = env.objc.borrow::<UITabBarControllerHostObject>(this);
    host_object
        .view_controllers
        .get(host_object.selected_index as usize)
        .copied()
        .unwrap_or(nil)
}

/// Replaces `old` (if it's showing) with the selected view controller's view,
/// above the tab bar, once this controller's own view is loaded. Only a
/// change of tab after that is an appearance (`notify`); otherwise it's the
/// controller's own appearance that its tabs are told about.
fn show_selected(env: &mut Environment, this: id, old: id, notify: bool) {
    if env.objc.borrow::<UIViewControllerHostObject>(this).view == nil {
        return;
    }
    let new = selected(env, this);
    if old != nil && old != new {
        let old_view: id = msg![env; old view];
        if notify {
            () = msg![env; old viewWillDisappear:false];
        }
        () = msg![env; old_view removeFromSuperview];
        if notify {
            () = msg![env; old viewDidDisappear:false];
        }
    }
    let view: id = msg![env; this view];
    let bounds: CGRect = msg![env; view bounds];
    let tab_bar: id = msg![env; this tabBar];
    let tab_bar_frame = CGRect {
        origin: CGPoint {
            x: 0.0,
            y: bounds.size.height - TAB_BAR_HEIGHT,
        },
        size: CGSize {
            width: bounds.size.width,
            height: TAB_BAR_HEIGHT,
        },
    };
    () = msg![env; tab_bar setFrame:tab_bar_frame];
    let tab_bar_superview: id = msg![env; tab_bar superview];
    if tab_bar_superview != view {
        () = msg![env; view addSubview:tab_bar];
    }
    if new == nil || new == old {
        return;
    }
    let new_view: id = msg![env; new view];
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: bounds.size.width,
            height: bounds.size.height - TAB_BAR_HEIGHT,
        },
    };
    () = msg![env; new_view setFrame:frame];
    if notify {
        () = msg![env; new viewWillAppear:false];
    }
    () = msg![env; view insertSubview:new_view atIndex:0i32];
    if notify {
        () = msg![env; new viewDidAppear:false];
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITabBarController: UIViewController

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITabBarControllerHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let host_object = std::mem::take(&mut env.objc.borrow_mut::<UITabBarControllerHostObject>(this).view_controllers);
    for vc in host_object {
        release(env, vc);
    }
    let tab_bar = env.objc.borrow::<UITabBarControllerHostObject>(this).tab_bar;
    release(env, tab_bar);
    msg_super![env; this dealloc]
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UITabBar");
    if msg![env; coder containsValueForKey:key] {
        let tab_bar: id = msg![env; coder decodeObjectForKey:key];
        retain(env, tab_bar);
        env.objc.borrow_mut::<UITabBarControllerHostObject>(this).tab_bar = tab_bar;
    }
    let key = get_static_str(env, "UIViewControllers");
    if msg![env; coder containsValueForKey:key] {
        let controllers: id = msg![env; coder decodeObjectForKey:key];
        () = msg![env; this setViewControllers:controllers];
    } else {
        log!("TODO: [(UITabBarController*){:?} initWithCoder:]: no UIViewControllers", this);
    }
    this
}

- (())loadView {
    () = msg_super![env; this loadView];
    show_selected(env, this, nil, false);
}

// The selected tab appears and disappears with this controller.
- (())viewWillAppear:(bool)animated {
    () = msg_super![env; this viewWillAppear:animated];
    let vc = selected(env, this);
    () = msg![env; vc viewWillAppear:animated];
}
- (())viewDidAppear:(bool)animated {
    () = msg_super![env; this viewDidAppear:animated];
    let vc = selected(env, this);
    () = msg![env; vc viewDidAppear:animated];
}
- (())viewWillDisappear:(bool)animated {
    () = msg_super![env; this viewWillDisappear:animated];
    let vc = selected(env, this);
    () = msg![env; vc viewWillDisappear:animated];
}
- (())viewDidDisappear:(bool)animated {
    () = msg_super![env; this viewDidDisappear:animated];
    let vc = selected(env, this);
    () = msg![env; vc viewDidDisappear:animated];
}

- (id)tabBar {
    let tab_bar = env.objc.borrow::<UITabBarControllerHostObject>(this).tab_bar;
    if tab_bar != nil {
        return tab_bar;
    }
    let tab_bar: id = msg_class![env; UITabBar new];
    let black: id = msg_class![env; UIColor blackColor];
    () = msg![env; tab_bar setBackgroundColor:black];
    env.objc.borrow_mut::<UITabBarControllerHostObject>(this).tab_bar = tab_bar;
    tab_bar
}

- (id)viewControllers {
    let vcs = env.objc.borrow::<UITabBarControllerHostObject>(this).view_controllers.clone();
    for &vc in &vcs {
        retain(env, vc);
    }
    let array = ns_array::from_vec(env, vcs);
    autorelease(env, array)
}
- (())setViewControllers:(id)controllers { // NSArray*
    () = msg![env; this setViewControllers:controllers animated:false];
}
- (())setViewControllers:(id)controllers // NSArray*
                animated:(bool)_animated {
    let old = selected(env, this);
    let count: NSUInteger = if controllers == nil { 0 } else { msg![env; controllers count] };
    let mut new = Vec::new();
    for i in 0..count {
        let vc: id = msg![env; controllers objectAtIndex:i];
        retain(env, vc);
        env.objc.borrow_mut::<UIViewControllerHostObject>(vc).tab_bar_controller = this;
        new.push(vc);
    }
    let host_object = env.objc.borrow_mut::<UITabBarControllerHostObject>(this);
    let old_vcs = std::mem::replace(&mut host_object.view_controllers, new);
    if host_object.selected_index >= count {
        host_object.selected_index = 0;
    }
    show_selected(env, this, old, true);
    for vc in old_vcs {
        release(env, vc);
    }
}

- (NSUInteger)selectedIndex {
    env.objc.borrow::<UITabBarControllerHostObject>(this).selected_index
}
- (())setSelectedIndex:(NSUInteger)index {
    let count = env.objc.borrow::<UITabBarControllerHostObject>(this).view_controllers.len();
    if index as usize >= count {
        return;
    }
    let old = selected(env, this);
    env.objc.borrow_mut::<UITabBarControllerHostObject>(this).selected_index = index;
    show_selected(env, this, old, true);
}
- (id)selectedViewController {
    selected(env, this)
}
- (())setSelectedViewController:(id)vc { // UIViewController*
    let vcs = &env.objc.borrow::<UITabBarControllerHostObject>(this).view_controllers;
    if let Some(index) = vcs.iter().position(|&v| v == vc) {
        () = msg![env; this setSelectedIndex:(index as NSUInteger)];
    }
}

// weak/non-retaining
- (id)delegate {
    env.objc.borrow::<UITabBarControllerHostObject>(this).delegate
}
- (())setDelegate:(id)delegate { // id<UITabBarControllerDelegate>
    env.objc.borrow_mut::<UITabBarControllerHostObject>(this).delegate = delegate;
}

- (())setCustomizableViewControllers:(id)controllers { // NSArray*
    todo_objc_setter!(this, controllers);
}

// Like a navigation controller, it rotates as its selected tab allows.
- (bool)shouldAutorotateToInterfaceOrientation:(UIInterfaceOrientation)orientation {
    let vc = selected(env, this);
    if vc == nil {
        return msg_super![env; this shouldAutorotateToInterfaceOrientation:orientation];
    }
    msg![env; vc shouldAutorotateToInterfaceOrientation:orientation]
}

@end

// TODO: draw the items and let them be tapped.
@implementation UITabBar: UIView

- (())setItems:(id)items animated:(bool)_animated { // NSArray*
    todo_objc_setter!(this, items);
}
- (())setItems:(id)items { // NSArray*
    todo_objc_setter!(this, items);
}

@end

@implementation UITabBarItem: UIBarItem

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITabBarItemHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title // NSString*
              image:(id)image // UIImage*
                tag:(NSInteger)tag {
    () = msg![env; this setTitle:title];
    () = msg![env; this setImage:image];
    () = msg![env; this setTag:tag];
    this
}
- (id)initWithTabBarSystemItem:(NSInteger)system_item
                           tag:(NSInteger)tag {
    log!("TODO: [(UITabBarItem*){:?} initWithTabBarSystemItem:{}]: no title or image", this, system_item);
    () = msg![env; this setTag:tag];
    this
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let key = get_static_str(env, "UIIsSystemItem");
    if msg![env; coder decodeBoolForKey:key] {
        let key = get_static_str(env, "UISystemItem");
        let system_item: NSInteger = msg![env; coder decodeIntegerForKey:key];
        log!("TODO: [(UITabBarItem*){:?} initWithCoder:]: system item {}, no title or image", this, system_item);
    }
    for (key, setter) in [("UITitle", "setTitle:"), ("UIImage", "setImage:"), ("UIBadgeValue", "setBadgeValue:")] {
        let key = get_static_str(env, key);
        if msg![env; coder containsValueForKey:key] {
            let value: id = msg![env; coder decodeObjectForKey:key];
            let sel = env.objc.lookup_selector(setter).unwrap();
            () = msg_send(env, (this, sel, value));
        }
    }
    let key = get_static_str(env, "UITag");
    let tag: NSInteger = msg![env; coder decodeIntegerForKey:key];
    () = msg![env; this setTag:tag];
    let key = get_static_str(env, "UIEnabled");
    if msg![env; coder containsValueForKey:key] {
        let enabled: bool = msg![env; coder decodeBoolForKey:key];
        () = msg![env; this setEnabled:enabled];
    }
    this
}

- (())dealloc {
    let host_object = env.objc.borrow::<UITabBarItemHostObject>(this);
    let (image, badge_value) = (host_object.image, host_object.badge_value);
    let title = env.objc.borrow::<UIBarItemHostObject>(this).title;
    release(env, image);
    release(env, badge_value);
    release(env, title);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)image {
    env.objc.borrow::<UITabBarItemHostObject>(this).image
}
- (())setImage:(id)image { // UIImage*
    retain(env, image);
    let old = std::mem::replace(&mut env.objc.borrow_mut::<UITabBarItemHostObject>(this).image, image);
    release(env, old);
}

- (id)badgeValue {
    env.objc.borrow::<UITabBarItemHostObject>(this).badge_value
}
- (())setBadgeValue:(id)value { // NSString*
    let value: id = msg![env; value copy];
    let old = std::mem::replace(&mut env.objc.borrow_mut::<UITabBarItemHostObject>(this).badge_value, value);
    release(env, old);
}

@end

};
