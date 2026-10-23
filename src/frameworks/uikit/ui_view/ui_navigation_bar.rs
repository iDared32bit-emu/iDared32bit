/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UINavigationBar`.

use super::ui_toolbar::{
    draw_bar_background, draw_button, UIBarStyle, UIBarStyleBlack, UIBarStyleBlackTranslucent,
    UIBarStyleDefault, BUTTON_HEIGHT,
};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{ns_array, NSUInteger};
use crate::frameworks::uikit::ui_bar_item::ui_bar_button_item::{
    UIBarButtonItemHostObject, UIBarButtonItemStyleBordered, UIBarButtonItemStylePlain,
};
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::frameworks::uikit::ui_navigation_item::UINavigationItemHostObject;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes,
    release, retain, ClassExports, NSZonePtr,
};
use crate::Environment;

pub struct UINavigationBarHostObject {
    superclass: super::UIViewHostObject,
    /// `UINavigationItem*`s, the top one last.
    items: Vec<id>,
    bar_style: UIBarStyle,
    /// Weak reference.
    delegate: id,
    title_label: id,
    /// The views currently shown for the top item, other than the title.
    item_views: Vec<id>,
    /// The `UIBarButtonItem*` made for the back button, if any.
    back_button: id,
}
impl_HostObject_with_superclass!(UINavigationBarHostObject);
impl Default for UINavigationBarHostObject {
    fn default() -> Self {
        Self {
            superclass: Default::default(),
            items: Vec::new(),
            bar_style: UIBarStyleDefault,
            delegate: nil,
            title_label: nil,
            item_views: Vec::new(),
            back_button: nil,
        }
    }
}

/// Height of a navigation bar, as on iPhone OS 3.
const BAR_HEIGHT: CGFloat = 44.0;
/// Space between the bar's edges and its buttons.
const EDGE_INSET: CGFloat = 5.0;

fn setup(env: &mut Environment, this: id) {
    // As on iPhone OS 3: bold white text.
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:(20.0 as CGFloat)];
    let white: id = msg_class![env; UIColor whiteColor];
    let clear: id = msg_class![env; UIColor clearColor];
    let label: id = msg_class![env; UILabel new];
    () = msg![env; label setFont:font];
    () = msg![env; label setTextColor:white];
    () = msg![env; label setBackgroundColor:clear];
    () = msg![env; label setTextAlignment:UITextAlignmentCenter];
    () = msg![env; this addSubview:label];
    env.objc
        .borrow_mut::<UINavigationBarHostObject>(this)
        .title_label = label;
    () = msg![env; this setOpaque:false];
}

/// Asks the delegate, if it implements `selector`, whether to go ahead.
fn delegate_allows(env: &mut Environment, this: id, selector: &str, item: id) -> bool {
    let delegate = env.objc.borrow::<UINavigationBarHostObject>(this).delegate;
    let sel = env
        .objc
        .register_host_selector(selector.to_string(), &mut env.mem);
    if delegate == nil || !msg![env; delegate respondsToSelector:sel] {
        return true;
    }
    crate::objc::msg_send(env, (delegate, sel, this, item))
}

fn tell_delegate(env: &mut Environment, this: id, selector: &str, item: id) {
    let delegate = env.objc.borrow::<UINavigationBarHostObject>(this).delegate;
    let sel = env
        .objc
        .register_host_selector(selector.to_string(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = crate::objc::msg_send(env, (delegate, sel, this, item));
    }
}

/// Places a bar button item's view at `x` (its left or right edge), and
/// returns its frame.
fn place_item(env: &mut Environment, this: id, item: id, x: CGFloat, from_right: bool) -> CGRect {
    let bounds: CGRect = msg![env; this bounds];
    let view: id = msg![env; item customView];
    let width: CGFloat = msg![env; item width];
    let width = if width > 0.0 {
        width
    } else {
        let size = bounds.size;
        let size: CGSize = msg![env; item sizeThatFits:size];
        size.width
    };
    let label: id = msg![env; item label];
    let style = env.objc.borrow::<UIBarButtonItemHostObject>(item).style;
    let bordered = label != nil && style != UIBarButtonItemStylePlain;
    let height = if bordered {
        BUTTON_HEIGHT
    } else {
        bounds.size.height - 8.0
    };
    let x = if from_right { x - width } else { x };
    // Whole points, as in UIKit, so the button images aren't blurred.
    let frame = CGRect {
        origin: CGPoint {
            x: x.round(),
            y: ((bounds.size.height - height) / 2.0).round(),
        },
        size: CGSize {
            width: width.round(),
            height,
        },
    };
    () = msg![env; view setFrame:frame];
    if label != nil {
        let view_bounds: CGRect = msg![env; view bounds];
        () = msg![env; label setFrame:view_bounds];
    }
    if bordered {
        let bar_style = env.objc.borrow::<UINavigationBarHostObject>(this).bar_style;
        draw_button(env, view, bar_style, style);
    }
    () = msg![env; this addSubview:view];
    retain(env, view);
    env.objc
        .borrow_mut::<UINavigationBarHostObject>(this)
        .item_views
        .push(view);
    frame
}

/// The back button for `item`, the one below the top item. It's a bordered
/// button with the item's title, not arrow-shaped as on iPhone OS 3.
fn make_back_button(env: &mut Environment, this: id, item: id) -> id {
    let back: id = msg![env; item backBarButtonItem];
    let title: id = if back != nil {
        msg![env; back title]
    } else {
        msg![env; item title]
    };
    let title = if title == nil {
        get_static_str(env, "Back")
    } else {
        title
    };
    // The bar handles taps itself, so the button doesn't retain it.
    let action = env
        .objc
        .register_host_selector("_touchHLE_popItem".to_string(), &mut env.mem);
    let button: id = msg_class![env; UIBarButtonItem alloc];
    let button: id = msg![env; button initWithTitle:title
                                               style:UIBarButtonItemStyleBordered
                                              target:nil
                                              action:action];
    env.objc
        .borrow_mut::<UIBarButtonItemHostObject>(button)
        .action = None;
    let view: id = msg![env; button customView];
    let touch_up_inside =
        crate::frameworks::uikit::ui_view::ui_control::UIControlEventTouchUpInside;
    () = msg![env; view addTarget:this action:action forControlEvents:touch_up_inside];
    button
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UINavigationBar: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UINavigationBarHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    setup(env, this);
    () = msg![env; this setBarStyle:UIBarStyleDefault];
    this
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    setup(env, this);
    let key = get_static_str(env, "UIBarStyle");
    let style: i32 = msg![env; coder decodeIntForKey:key];
    () = msg![env; this setBarStyle:(style as UIBarStyle)];
    let key = get_static_str(env, "UIItems");
    let items: id = msg![env; coder decodeObjectForKey:key];
    if items != nil {
        () = msg![env; this setItems:items];
    }
    this
}

- (())dealloc {
    let UINavigationBarHostObject {
        items,
        title_label,
        item_views,
        back_button,
        ..
    } = std::mem::take(env.objc.borrow_mut(this));
    for item in items {
        env.objc
            .borrow_mut::<UINavigationItemHostObject>(item)
            .navigation_bar = nil;
        release(env, item);
    }
    for view in item_views {
        release(env, view);
    }
    release(env, title_label);
    release(env, back_button);
    msg_super![env; this dealloc]
}

- (CGSize)sizeThatFits:(CGSize)size {
    CGSize { width: size.width, height: BAR_HEIGHT }
}

- (id)delegate {
    env.objc.borrow::<UINavigationBarHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).delegate = delegate;
}

- (UIBarStyle)barStyle {
    env.objc.borrow::<UINavigationBarHostObject>(this).bar_style
}
- (())setBarStyle:(UIBarStyle)style {
    match style {
        UIBarStyleDefault | UIBarStyleBlack | UIBarStyleBlackTranslucent => {}
        _ => unimplemented!("UIBarStyle {}", style),
    };
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).bar_style = style;
    // The background is drawn as an image (see layoutSubviews), and the
    // buttons are redrawn for the new style.
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; this setBackgroundColor:clear];
    let mut to_clear = env
        .objc
        .borrow::<UINavigationBarHostObject>(this)
        .item_views
        .clone();
    to_clear.push(this);
    for view in to_clear {
        let layer: id = msg![env; view layer];
        () = msg![env; layer setContents:nil];
    }
    () = msg![env; this setNeedsLayout];
}

- (id)items {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items.clone();
    for &item in &items {
        retain(env, item);
    }
    let res = ns_array::from_vec(env, items);
    autorelease(env, res)
}
- (())setItems:(id)items { // NSArray*
    let count: NSUInteger = msg![env; items count];
    let mut new_items = Vec::new();
    for i in 0..count {
        let item: id = msg![env; items objectAtIndex:i];
        retain(env, item);
        env.objc
            .borrow_mut::<UINavigationItemHostObject>(item)
            .navigation_bar = this;
        new_items.push(item);
    }
    let old_items = std::mem::replace(
        &mut env.objc.borrow_mut::<UINavigationBarHostObject>(this).items,
        new_items,
    );
    for item in old_items {
        let host = env.objc.borrow_mut::<UINavigationItemHostObject>(item);
        if host.navigation_bar == this {
            host.navigation_bar = nil;
        }
        release(env, item);
    }
    () = msg![env; this setNeedsLayout];
}
- (())setItems:(id)items animated:(bool)_animated {
    () = msg![env; this setItems:items];
}

- (id)topItem {
    let items = &env.objc.borrow::<UINavigationBarHostObject>(this).items;
    items.last().copied().unwrap_or(nil)
}
- (id)backItem {
    let items = &env.objc.borrow::<UINavigationBarHostObject>(this).items;
    items.iter().rev().nth(1).copied().unwrap_or(nil)
}

- (())pushNavigationItem:(id)item animated:(bool)_animated {
    if !delegate_allows(env, this, "navigationBar:shouldPushItem:", item) {
        return;
    }
    retain(env, item);
    env.objc
        .borrow_mut::<UINavigationItemHostObject>(item)
        .navigation_bar = this;
    env.objc
        .borrow_mut::<UINavigationBarHostObject>(this)
        .items
        .push(item);
    () = msg![env; this setNeedsLayout];
    tell_delegate(env, this, "navigationBar:didPushItem:", item);
}

- (id)popNavigationItemAnimated:(bool)_animated {
    let item: id = msg![env; this topItem];
    if item == nil || !delegate_allows(env, this, "navigationBar:shouldPopItem:", item) {
        return nil;
    }
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).items.pop();
    env.objc
        .borrow_mut::<UINavigationItemHostObject>(item)
        .navigation_bar = nil;
    () = msg![env; this setNeedsLayout];
    tell_delegate(env, this, "navigationBar:didPopItem:", item);
    autorelease(env, item)
}

- (())_touchHLE_popItem {
    let _: id = msg![env; this popNavigationItemAnimated:true];
}

- (())layoutSubviews {
    let bar_style = env.objc.borrow::<UINavigationBarHostObject>(this).bar_style;
    draw_bar_background(env, this, bar_style, false);

    let host = env.objc.borrow_mut::<UINavigationBarHostObject>(this);
    let old_views = std::mem::take(&mut host.item_views);
    let old_back_button = std::mem::replace(&mut host.back_button, nil);
    let title_label = host.title_label;
    for view in old_views {
        () = msg![env; view removeFromSuperview];
        release(env, view);
    }
    release(env, old_back_button);

    let top: id = msg![env; this topItem];
    if top == nil {
        () = msg![env; title_label setText:nil];
        return;
    }
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UINavigationItemHostObject>(top);
    let (left, right, title_view, hides_back) = (
        host.left_bar_button_item,
        host.right_bar_button_item,
        host.title_view,
        host.hides_back_button,
    );

    let back: id = msg![env; this backItem];
    let left = if left == nil && back != nil && !hides_back {
        let button = make_back_button(env, this, back);
        env.objc.borrow_mut::<UINavigationBarHostObject>(this).back_button = button;
        button
    } else {
        left
    };

    // The space left between the buttons for the title.
    let mut min_x = EDGE_INSET;
    let mut max_x = bounds.size.width - EDGE_INSET;
    if left != nil {
        let frame = place_item(env, this, left, EDGE_INSET, false);
        min_x = frame.origin.x + frame.size.width + EDGE_INSET;
    }
    if right != nil {
        let x = bounds.size.width - EDGE_INSET;
        let frame = place_item(env, this, right, x, true);
        max_x = frame.origin.x - EDGE_INSET;
    }

    // Centered on the bar if there's room, else in the space left.
    let center = |width: CGFloat| {
        let width = width.min((max_x - min_x).max(0.0));
        let x = ((bounds.size.width - width) / 2.0).clamp(min_x, (max_x - width).max(min_x));
        (x.round(), width)
    };
    if title_view != nil {
        let frame: CGRect = msg![env; title_view frame];
        let (x, width) = center(frame.size.width);
        let frame = CGRect {
            origin: CGPoint {
                x,
                y: ((bounds.size.height - frame.size.height) / 2.0).round(),
            },
            size: CGSize { width, ..frame.size },
        };
        () = msg![env; title_view setFrame:frame];
        () = msg![env; this addSubview:title_view];
        retain(env, title_view);
        env.objc
            .borrow_mut::<UINavigationBarHostObject>(this)
            .item_views
            .push(title_view);
        () = msg![env; title_label setText:nil];
        return;
    }
    let title: id = msg![env; top title];
    () = msg![env; title_label setText:title];
    let size = bounds.size;
    let size: CGSize = msg![env; title_label sizeThatFits:size];
    let (x, width) = center(size.width);
    let frame = CGRect {
        origin: CGPoint { x, y: 0.0 },
        size: CGSize {
            width,
            height: bounds.size.height,
        },
    };
    () = msg![env; title_label setFrame:frame];
}

@end

};
