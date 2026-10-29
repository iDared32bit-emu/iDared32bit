/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UISegmentedControl`.

use super::{send_actions, UIControlEventValueChanged, UIControlHostObject};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::{ns_string, NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::frameworks::uikit::ui_view::ui_toolbar::draw_segmented_control;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr,
};
use crate::Environment;

type UISegmentedControlStyle = NSInteger;
const UISegmentedControlStylePlain: UISegmentedControlStyle = 0;

const UISegmentedControlNoSegment: NSInteger = -1;

#[derive(Default)]
pub struct UISegmentedControlHostObject {
    superclass: UIControlHostObject,
    /// Each segment's title (`NSString*`) or image (`UIImage*`).
    contents: Vec<id>,
    /// The `UILabel*` or `UIImageView*` showing each segment's contents.
    content_views: Vec<id>,
    selected: NSInteger,
    momentary: bool,
    style: UISegmentedControlStyle,
}
impl_HostObject_with_superclass!(UISegmentedControlHostObject);

/// Undocumented class for each segment in a nib.
#[derive(Default)]
pub struct UISegmentHostObject {
    superclass: UIControlHostObject,
    /// The title (`NSString*`) or image (`UIImage*`).
    info: id,
}
impl_HostObject_with_superclass!(UISegmentHostObject);

fn segment_count(env: &mut Environment, this: id) -> usize {
    env.objc
        .borrow::<UISegmentedControlHostObject>(this)
        .contents
        .len()
}

/// Makes the view that shows a segment's title or image.
fn make_content_view(env: &mut Environment, content: id) -> id {
    let string_class: id = msg_class![env; NSString class];
    let view: id = if msg![env; content isKindOfClass:string_class] {
        let label: id = msg_class![env; UILabel new];
        () = msg![env; label setText:content];
        let font: id = msg_class![env; UIFont boldSystemFontOfSize:(13.0 as CGFloat)];
        () = msg![env; label setFont:font];
        () = msg![env; label setTextAlignment:UITextAlignmentCenter];
        let clear: id = msg_class![env; UIColor clearColor];
        () = msg![env; label setBackgroundColor:clear];
        label
    } else {
        msg![env; (msg_class![env; UIImageView alloc]) initWithImage:content]
    };
    // Taps go to the control.
    () = msg![env; view setUserInteractionEnabled:false];
    view
}

fn insert_segment(env: &mut Environment, this: id, content: id, index: NSUInteger) {
    retain(env, content);
    let view = make_content_view(env, content);
    () = msg![env; this addSubview:view];
    let host = env.objc.borrow_mut::<UISegmentedControlHostObject>(this);
    let index = (index as usize).min(host.contents.len());
    host.contents.insert(index, content);
    host.content_views.insert(index, view);
    if host.selected >= index as NSInteger {
        host.selected += 1;
    }
    () = msg![env; this setNeedsLayout];
}

fn remove_segment(env: &mut Environment, this: id, index: usize) {
    let host = env.objc.borrow_mut::<UISegmentedControlHostObject>(this);
    if index >= host.contents.len() {
        return;
    }
    let content = host.contents.remove(index);
    let view = host.content_views.remove(index);
    let selected = host.selected;
    if selected == index as NSInteger {
        host.selected = UISegmentedControlNoSegment;
    } else if selected > index as NSInteger {
        host.selected -= 1;
    }
    () = msg![env; view removeFromSuperview];
    release(env, view);
    release(env, content);
    () = msg![env; this setNeedsLayout];
}

fn set_content(env: &mut Environment, this: id, content: id, index: NSUInteger) {
    let count = segment_count(env, this);
    if (index as usize) >= count || content == nil {
        return;
    }
    remove_segment(env, this, index as usize);
    let selected = env
        .objc
        .borrow::<UISegmentedControlHostObject>(this)
        .selected;
    insert_segment(env, this, content, index);
    // Replacing a segment keeps it selected.
    env.objc
        .borrow_mut::<UISegmentedControlHostObject>(this)
        .selected = selected;
}

fn content_at(env: &mut Environment, this: id, index: NSUInteger, want_string: bool) -> id {
    let host = env.objc.borrow::<UISegmentedControlHostObject>(this);
    let Some(&content) = host.contents.get(index as usize) else {
        return nil;
    };
    let string_class: id = msg_class![env; NSString class];
    let is_string: bool = msg![env; content isKindOfClass:string_class];
    if is_string == want_string {
        content
    } else {
        nil
    }
}

/// Which segment the point (in the control's coordinates) is in.
fn segment_at(env: &mut Environment, this: id, point: CGPoint) -> Option<usize> {
    let count = segment_count(env, this);
    let bounds: CGRect = msg![env; this bounds];
    if count == 0 || bounds.size.width <= 0.0 {
        return None;
    }
    let i = (point.x / bounds.size.width * count as CGFloat).floor();
    Some((i.max(0.0) as usize).min(count - 1))
}

fn select(env: &mut Environment, this: id, index: NSInteger) {
    env.objc
        .borrow_mut::<UISegmentedControlHostObject>(this)
        .selected = index;
    () = msg![env; this setNeedsLayout];
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UISegmentedControl: UIControl

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UISegmentedControlHostObject {
        selected: UISegmentedControlNoSegment,
        style: UISegmentedControlStylePlain,
        ..Default::default()
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithItems:(id)items { // NSArray*
    let this: id = msg![env; this initWithFrame:(CGRect::default())];
    let count: NSUInteger = if items == nil { 0 } else { msg![env; items count] };
    for i in 0..count {
        let item: id = msg![env; items objectAtIndex:i];
        insert_segment(env, this, item, i);
    }
    // Real ones are as wide as their segments need; 44pt tall, like the
    // nibs' plain ones.
    let width = (count as CGFloat * 60.0).max(60.0);
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width, height: 44.0 },
    };
    () = msg![env; this setFrame:frame];
    this
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = ns_string::get_static_str(env, "UISegments");
    let segments: id = msg![env; coder decodeObjectForKey:key];
    let count: NSUInteger = if segments == nil { 0 } else { msg![env; segments count] };
    for i in 0..count {
        let segment: id = msg![env; segments objectAtIndex:i];
        // The control draws its segments itself.
        () = msg![env; segment removeFromSuperview];
        let info = env.objc.borrow::<UISegmentHostObject>(segment).info;
        if info != nil {
            insert_segment(env, this, info, i);
        } else {
            log!("Warning: segment {} of {:?} has no title or image", i, this);
        }
    }
    let key = ns_string::get_static_str(env, "UISelectedSegmentIndex");
    if msg![env; coder containsValueForKey:key] {
        let selected: NSInteger = msg![env; coder decodeIntegerForKey:key];
        select(env, this, selected);
    }
    let key = ns_string::get_static_str(env, "UISegmentedControlStyle");
    let style: NSInteger = msg![env; coder decodeIntegerForKey:key];
    () = msg![env; this setSegmentedControlStyle:style];
    let key = ns_string::get_static_str(env, "UIMomentary");
    let momentary: bool = msg![env; coder decodeBoolForKey:key];
    () = msg![env; this setMomentary:momentary];
    this
}

- (())dealloc {
    let host = env.objc.borrow_mut::<UISegmentedControlHostObject>(this);
    let contents = std::mem::take(&mut host.contents);
    let views = std::mem::take(&mut host.content_views);
    for object in contents.into_iter().chain(views) {
        release(env, object);
    }
    msg_super![env; this dealloc]
}

- (NSUInteger)numberOfSegments {
    segment_count(env, this) as NSUInteger
}

- (NSInteger)selectedSegmentIndex {
    env.objc.borrow::<UISegmentedControlHostObject>(this).selected
}
// Unlike a tap, this doesn't send UIControlEventValueChanged.
- (())setSelectedSegmentIndex:(NSInteger)index {
    let count = segment_count(env, this) as NSInteger;
    let index = if (0..count).contains(&index) {
        index
    } else {
        UISegmentedControlNoSegment
    };
    select(env, this, index);
}

- (bool)isMomentary {
    env.objc.borrow::<UISegmentedControlHostObject>(this).momentary
}
- (())setMomentary:(bool)momentary {
    env.objc.borrow_mut::<UISegmentedControlHostObject>(this).momentary = momentary;
}

- (UISegmentedControlStyle)segmentedControlStyle {
    env.objc.borrow::<UISegmentedControlHostObject>(this).style
}
- (())setSegmentedControlStyle:(UISegmentedControlStyle)style {
    if style != UISegmentedControlStylePlain {
        log!("TODO: UISegmentedControl style {} is drawn as the plain style", style);
    }
    env.objc.borrow_mut::<UISegmentedControlHostObject>(this).style = style;
}

- (id)titleForSegmentAtIndex:(NSUInteger)index {
    content_at(env, this, index, true)
}
- (())setTitle:(id)title forSegmentAtIndex:(NSUInteger)index { // NSString*
    set_content(env, this, title, index);
}
- (id)imageForSegmentAtIndex:(NSUInteger)index {
    content_at(env, this, index, false)
}
- (())setImage:(id)image forSegmentAtIndex:(NSUInteger)index { // UIImage*
    set_content(env, this, image, index);
}

- (())insertSegmentWithTitle:(id)title // NSString*
                     atIndex:(NSUInteger)index
                    animated:(bool)_animated {
    insert_segment(env, this, title, index);
}
- (())insertSegmentWithImage:(id)image // UIImage*
                     atIndex:(NSUInteger)index
                    animated:(bool)_animated {
    insert_segment(env, this, image, index);
}
- (())removeSegmentAtIndex:(NSUInteger)index animated:(bool)_animated {
    remove_segment(env, this, index as usize);
}
- (())removeAllSegments {
    while segment_count(env, this) > 0 {
        remove_segment(env, this, 0);
    }
}

- (())layoutSubviews {
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UISegmentedControlHostObject>(this);
    let views = host.content_views.clone();
    let selected = usize::try_from(host.selected).ok();
    let count = views.len();
    for (i, &view) in views.iter().enumerate() {
        let x0 = bounds.size.width * i as CGFloat / count as CGFloat;
        let x1 = bounds.size.width * (i + 1) as CGFloat / count as CGFloat;
        let label_class: id = msg_class![env; UILabel class];
        let frame = if msg![env; view isKindOfClass:label_class] {
            let color: id = if selected == Some(i) {
                msg_class![env; UIColor whiteColor]
            } else {
                msg_class![env; UIColor colorWithWhite:(0.3 as CGFloat) alpha:(1.0 as CGFloat)]
            };
            () = msg![env; view setTextColor:color];
            CGRect {
                origin: CGPoint { x: x0, y: 0.0 },
                size: CGSize { width: x1 - x0, height: bounds.size.height },
            }
        } else {
            // Images keep their size, centred in the segment.
            let view_frame: CGRect = msg![env; view frame];
            let size = view_frame.size;
            CGRect {
                origin: CGPoint {
                    x: ((x0 + x1 - size.width) / 2.0).round(),
                    y: ((bounds.size.height - size.height) / 2.0).round(),
                },
                size,
            }
        };
        () = msg![env; view setFrame:frame];
    }
    draw_segmented_control(env, this, count, selected);
}

- (())touchesBegan:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    () = msg_super![env; this touchesBegan:touches withEvent:event];
    if !msg![env; this isEnabled] {
        return;
    }
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    let Some(index) = segment_at(env, this, point) else {
        return;
    };
    let host = env.objc.borrow::<UISegmentedControlHostObject>(this);
    if host.selected == index as NSInteger && !host.momentary {
        return;
    }
    select(env, this, index as NSInteger);
    send_actions(env, this, event, UIControlEventValueChanged);
}

- (())touchesEnded:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    () = msg_super![env; this touchesEnded:touches withEvent:event];
    // A momentary control only shows the segment while it's held.
    if env.objc.borrow::<UISegmentedControlHostObject>(this).momentary {
        select(env, this, UISegmentedControlNoSegment);
    }
}

@end

// Undocumented class used by UISegmentedControl's nibs. Only its title or
// image is used: the control draws the segments itself.
@implementation UISegment: UIControl

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UISegmentHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = ns_string::get_static_str(env, "UISegmentInfo");
    let info: id = msg![env; coder decodeObjectForKey:key];
    retain(env, info);
    env.objc.borrow_mut::<UISegmentHostObject>(this).info = info;
    this
}

- (())dealloc {
    let info = std::mem::take(&mut env.objc.borrow_mut::<UISegmentHostObject>(this).info);
    release(env, info);
    msg_super![env; this dealloc]
}

@end

};
