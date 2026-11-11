/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITabBarController`, `UITabBar` and `UITabBarItem`.
//!
//! The bar draws each item's icon (only its alpha counts, as on iOS) and
//! title, and switches tabs when one is tapped.
//!
//! TODO: system items have no title or image, there's no "More" tab for
//! over five tabs, and the bar is drawn plainer than iOS's.

use super::UIViewControllerHostObject;
use crate::frameworks::core_graphics::cg_image::{self, CGImageRef, CGImageRelease};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{ns_array, NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_application::UIInterfaceOrientation;
use crate::frameworks::uikit::ui_bar_item::UIBarItemHostObject;
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::frameworks::uikit::ui_view::UIViewHostObject;
use crate::image::Image;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_send, msg_super, nil,
    objc_classes, release, retain, todo_objc_setter, ClassExports, NSZonePtr,
};
use crate::Environment;

const TAB_BAR_HEIGHT: CGFloat = 49.0;
/// Without a "More" tab, the bar has room for this many.
const MAX_ITEMS: usize = 5;

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

#[derive(Default)]
struct UITabBarHostObject {
    superclass: UIViewHostObject,
    /// `UITabBarItem*`s, retained.
    items: Vec<id>,
    /// Weak.
    selected_item: id,
    /// Weak.
    delegate: id,
    /// A view per item, drawn by [update_tab_bar]. Retained.
    cells: Vec<id>,
}
impl_HostObject_with_superclass!(UITabBarHostObject);

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
    sync_tab_bar(env, this, tab_bar);
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

/// Gives the bar the tabs' items, and shows the selected one.
fn sync_tab_bar(env: &mut Environment, this: id, tab_bar: id) {
    let vcs = env
        .objc
        .borrow::<UITabBarControllerHostObject>(this)
        .view_controllers
        .clone();
    let mut items = Vec::new();
    for vc in vcs {
        let item: id = msg![env; vc tabBarItem];
        retain(env, item);
        items.push(item);
    }
    let items = ns_array::from_vec(env, items);
    () = msg![env; tab_bar setItems:items animated:false];
    release(env, items);
    let vc = selected(env, this);
    let item: id = if vc == nil {
        nil
    } else {
        msg![env; vc tabBarItem]
    };
    () = msg![env; tab_bar setSelectedItem:item];
    () = msg![env; tab_bar setDelegate:this];
}

/// A copy of a tab icon in one colour: iOS only uses the icon's alpha.
fn tinted_icon(env: &mut Environment, image: id, (r, g, b): (f32, f32, f32)) -> id {
    if image == nil {
        return nil;
    }
    let cg_image: CGImageRef = msg![env; image CGImage];
    let source = cg_image::borrow_image(&env.objc, cg_image);
    let dimensions = source.dimensions();
    // The pixels are premultiplied.
    let pixels = source
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = p[3] as f32;
            [(r * a) as u8, (g * a) as u8, (b * a) as u8, p[3]]
        })
        .collect();
    let new = cg_image::from_image(env, Image::from_pixel_vec(pixels, dimensions));
    let scale: CGFloat = msg![env; image scale];
    let new_image: id = msg_class![env; UIImage imageWithCGImage:new scale:scale orientation:0];
    CGImageRelease(env, new);
    new_image
}

/// Rebuilds the views showing a tab bar's items.
fn update_tab_bar(env: &mut Environment, this: id) {
    let host_object = env.objc.borrow_mut::<UITabBarHostObject>(this);
    let old_cells = std::mem::take(&mut host_object.cells);
    let items: Vec<id> = host_object.items.iter().copied().take(MAX_ITEMS).collect();
    let selected_item = host_object.selected_item;
    for cell in old_cells {
        () = msg![env; cell removeFromSuperview];
        release(env, cell);
    }
    if items.is_empty() {
        return;
    }
    let bounds: CGRect = msg![env; this bounds];
    let width = bounds.size.width / items.len() as CGFloat;
    let mut cells = Vec::new();
    for (i, &item) in items.iter().enumerate() {
        let is_selected = item == selected_item;
        let frame = CGRect {
            origin: CGPoint {
                x: width * i as CGFloat + 2.0,
                y: 2.0,
            },
            size: CGSize {
                width: width - 4.0,
                height: TAB_BAR_HEIGHT - 4.0,
            },
        };
        let cell: id = msg_class![env; UIView alloc];
        let cell: id = msg![env; cell initWithFrame:frame];
        let background: id = if is_selected {
            msg_class![env; UIColor colorWithWhite:1.0f32 alpha:0.15f32]
        } else {
            msg_class![env; UIColor clearColor]
        };
        () = msg![env; cell setBackgroundColor:background];
        let layer: id = msg![env; cell layer];
        () = msg![env; layer setCornerRadius:3.0f32];

        let image: id = msg![env; item image];
        let tint = if is_selected {
            (0.4, 0.7, 1.0)
        } else {
            (0.6, 0.6, 0.6)
        };
        let icon = tinted_icon(env, image, tint);
        if icon != nil {
            let size: CGSize = msg![env; icon size];
            let icon_frame = CGRect {
                origin: CGPoint {
                    x: (frame.size.width - size.width) / 2.0,
                    y: 2.0,
                },
                size,
            };
            let icon_view: id = msg_class![env; UIImageView alloc];
            let icon_view: id = msg![env; icon_view initWithImage:icon];
            () = msg![env; icon_view setFrame:icon_frame];
            () = msg![env; cell addSubview:icon_view];
            release(env, icon_view);
        }

        let title: id = msg![env; item title];
        let label: id = msg_class![env; UILabel alloc];
        let label_frame = CGRect {
            origin: CGPoint {
                x: 0.0,
                y: frame.size.height - 13.0,
            },
            size: CGSize {
                width: frame.size.width,
                height: 12.0,
            },
        };
        let label: id = msg![env; label initWithFrame:label_frame];
        () = msg![env; label setText:title];
        let font: id = msg_class![env; UIFont boldSystemFontOfSize:10.0f32];
        () = msg![env; label setFont:font];
        () = msg![env; label setTextAlignment:UITextAlignmentCenter];
        let text_color: id = if is_selected {
            msg_class![env; UIColor whiteColor]
        } else {
            msg_class![env; UIColor lightGrayColor]
        };
        () = msg![env; label setTextColor:text_color];
        let clear: id = msg_class![env; UIColor clearColor];
        () = msg![env; label setBackgroundColor:clear];
        () = msg![env; cell addSubview:label];
        release(env, label);

        let badge_value: id = msg![env; item badgeValue];
        if badge_value != nil {
            let length: NSUInteger = msg![env; badge_value length];
            let badge_width = 18.0 + 7.0 * length.saturating_sub(1) as CGFloat;
            let badge_frame = CGRect {
                origin: CGPoint {
                    x: frame.size.width / 2.0 + 6.0,
                    y: 0.0,
                },
                size: CGSize {
                    width: badge_width,
                    height: 18.0,
                },
            };
            let badge: id = msg_class![env; UILabel alloc];
            let badge: id = msg![env; badge initWithFrame:badge_frame];
            () = msg![env; badge setText:badge_value];
            let font: id = msg_class![env; UIFont boldSystemFontOfSize:12.0f32];
            () = msg![env; badge setFont:font];
            () = msg![env; badge setTextAlignment:UITextAlignmentCenter];
            let white: id = msg_class![env; UIColor whiteColor];
            () = msg![env; badge setTextColor:white];
            let red: id = msg_class![env; UIColor redColor];
            () = msg![env; badge setBackgroundColor:red];
            let layer: id = msg![env; badge layer];
            () = msg![env; layer setCornerRadius:9.0f32];
            () = msg![env; cell addSubview:badge];
            release(env, badge);
        }

        () = msg![env; this addSubview:cell];
        cells.push(cell);
    }
    env.objc.borrow_mut::<UITabBarHostObject>(this).cells = cells;
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

// UITabBarDelegate implementation: a tab was tapped.
- (())tabBar:(id)tab_bar didSelectItem:(id)item { // UITabBar*, UITabBarItem*
    let vcs = env.objc.borrow::<UITabBarControllerHostObject>(this).view_controllers.clone();
    let mut index = None;
    for (i, &vc) in vcs.iter().enumerate() {
        let vc_item: id = msg![env; vc tabBarItem];
        if vc_item == item {
            index = Some(i);
        }
    }
    let Some(index) = index else {
        return;
    };
    let vc = vcs[index];
    let delegate = env.objc.borrow::<UITabBarControllerHostObject>(this).delegate;
    let should_sel = env.objc.lookup_selector("tabBarController:shouldSelectViewController:");
    let asks = delegate != nil && should_sel.is_some_and(|sel| env.objc.object_has_method(&env.mem, delegate, sel));
    if asks && !msg![env; delegate tabBarController:this shouldSelectViewController:vc] {
        // Put the bar back as it was.
        sync_tab_bar(env, this, tab_bar);
        return;
    }
    () = msg![env; this setSelectedIndex:(index as NSUInteger)];
    let did_sel = env.objc.lookup_selector("tabBarController:didSelectViewController:");
    if delegate != nil && did_sel.is_some_and(|sel| env.objc.object_has_method(&env.mem, delegate, sel)) {
        () = msg![env; delegate tabBarController:this didSelectViewController:vc];
    }
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

@implementation UITabBar: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITabBarHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let host_object = env.objc.borrow_mut::<UITabBarHostObject>(this);
    let items = std::mem::take(&mut host_object.items);
    let cells = std::mem::take(&mut host_object.cells);
    for object in items.into_iter().chain(cells) {
        release(env, object);
    }
    msg_super![env; this dealloc]
}

- (id)items {
    let items = env.objc.borrow::<UITabBarHostObject>(this).items.clone();
    for &item in &items {
        retain(env, item);
    }
    let array = ns_array::from_vec(env, items);
    autorelease(env, array)
}
- (())setItems:(id)items { // NSArray*
    () = msg![env; this setItems:items animated:false];
}
- (())setItems:(id)items animated:(bool)_animated { // NSArray*
    let count: NSUInteger = if items == nil { 0 } else { msg![env; items count] };
    let mut new = Vec::new();
    for i in 0..count {
        let item: id = msg![env; items objectAtIndex:i];
        retain(env, item);
        new.push(item);
    }
    let host_object = env.objc.borrow_mut::<UITabBarHostObject>(this);
    let old = std::mem::replace(&mut host_object.items, new);
    if !host_object.items.contains(&host_object.selected_item) {
        host_object.selected_item = nil;
    }
    for item in old {
        release(env, item);
    }
    update_tab_bar(env, this);
}

- (id)selectedItem {
    env.objc.borrow::<UITabBarHostObject>(this).selected_item
}
- (())setSelectedItem:(id)item { // UITabBarItem*
    env.objc.borrow_mut::<UITabBarHostObject>(this).selected_item = item;
    update_tab_bar(env, this);
}

// weak/non-retaining
- (id)delegate {
    env.objc.borrow::<UITabBarHostObject>(this).delegate
}
- (())setDelegate:(id)delegate { // id<UITabBarDelegate>
    env.objc.borrow_mut::<UITabBarHostObject>(this).delegate = delegate;
}

- (())setFrame:(CGRect)frame {
    () = msg_super![env; this setFrame:frame];
    update_tab_bar(env, this);
}

// Touches go to the bar, not the items' views.
- (id)hitTest:(CGPoint)point withEvent:(id)event { // UIEvent*
    if msg![env; this pointInside:point withEvent:event] {
        this
    } else {
        nil
    }
}

- (())touchesEnded:(id)touches withEvent:(id)_event { // NSSet* of UITouch*
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    let bounds: CGRect = msg![env; this bounds];
    let host_object = env.objc.borrow::<UITabBarHostObject>(this);
    let items: Vec<id> = host_object.items.iter().copied().take(MAX_ITEMS).collect();
    let (old_selected, delegate) = (host_object.selected_item, host_object.delegate);
    if items.is_empty() || bounds.size.width <= 0.0 {
        return;
    }
    let width = bounds.size.width / items.len() as CGFloat;
    let index = ((point.x - bounds.origin.x) / width).floor();
    if index < 0.0 || index as usize >= items.len() {
        return;
    }
    let item = items[index as usize];
    if !msg![env; item isEnabled] || item == old_selected {
        return;
    }
    () = msg![env; this setSelectedItem:item];
    if delegate != nil {
        () = msg![env; delegate tabBar:this didSelectItem:item];
    }
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
