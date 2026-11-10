/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UISlider`.
//!
//! Drawn with three image views: the track left of the thumb (the minimum
//! track), the track right of it (the maximum track) and the thumb. Without
//! custom images, they're plain coloured shapes.

use super::{send_actions, UIControlEventValueChanged, UIControlState, UIControlStateNormal};
use crate::environment::Environment;
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, todo_objc_setter, ClassExports, NSZonePtr,
};

// The default look's sizes, chosen to look like iPhone OS's.
const DEFAULT_TRACK_HEIGHT: CGFloat = 9.0;
const DEFAULT_THUMB_SIZE: CGFloat = 23.0;

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Thumb,
    MinimumTrack,
    MaximumTrack,
}

pub struct UISliderHostObject {
    superclass: super::UIControlHostObject,
    value: f32,
    minimum_value: f32,
    maximum_value: f32,
    continuous: bool,
    /// Custom images by part and state, retained.
    images: Vec<(Part, UIControlState, id)>,
    /// `UIImageView*`s
    minimum_track: id,
    maximum_track: id,
    thumb: id,
    /// While dragging: where on the thumb it was grabbed, from its centre.
    grab_offset: CGFloat,
    value_at_grab: f32,
}
impl_HostObject_with_superclass!(UISliderHostObject);
impl Default for UISliderHostObject {
    fn default() -> Self {
        UISliderHostObject {
            superclass: Default::default(),
            value: 0.0,
            minimum_value: 0.0,
            maximum_value: 1.0,
            continuous: true,
            images: Vec::new(),
            minimum_track: nil,
            maximum_track: nil,
            thumb: nil,
            grab_offset: 0.0,
            value_at_grab: 0.0,
        }
    }
}

fn init_common(env: &mut Environment, this: id) -> id {
    for part in [Part::MinimumTrack, Part::MaximumTrack, Part::Thumb] {
        let view: id = msg_class![env; UIImageView new];
        () = msg![env; this addSubview:view];
        let host_object = env.objc.borrow_mut::<UISliderHostObject>(this);
        match part {
            Part::MinimumTrack => host_object.minimum_track = view,
            Part::MaximumTrack => host_object.maximum_track = view,
            Part::Thumb => host_object.thumb = view,
        }
    }
    update(env, this);
    this
}

/// The image set for a part in the current state, or in the normal state.
fn image_for(env: &mut Environment, this: id, part: Part) -> id {
    let state: UIControlState = msg![env; this state];
    let images = &env.objc.borrow::<UISliderHostObject>(this).images;
    let find = |state| {
        images
            .iter()
            .find(|&&(p, s, _)| p == part && s == state)
            .map(|&(_, _, image)| image)
    };
    find(state)
        .or_else(|| find(UIControlStateNormal))
        .unwrap_or(nil)
}

fn set_image(env: &mut Environment, this: id, part: Part, state: UIControlState, image: id) {
    retain(env, image);
    let images = &mut env.objc.borrow_mut::<UISliderHostObject>(this).images;
    let old = images.iter().position(|&(p, s, _)| p == part && s == state);
    let old = old.map(|i| images.remove(i).2);
    if image != nil {
        images.push((part, state, image));
    }
    if let Some(old) = old {
        release(env, old);
    }
    update(env, this);
}

/// Shows the current images (or the default look) and lays them out.
fn update(env: &mut Environment, this: id) {
    let &UISliderHostObject {
        minimum_track,
        maximum_track,
        thumb,
        ..
    } = env.objc.borrow(this);
    for (part, view) in [
        (Part::MinimumTrack, minimum_track),
        (Part::MaximumTrack, maximum_track),
        (Part::Thumb, thumb),
    ] {
        let image = image_for(env, this, part);
        () = msg![env; view setImage:image];
        let color: id = if image != nil {
            msg_class![env; UIColor clearColor]
        } else {
            match part {
                Part::MinimumTrack => msg_class![env; UIColor colorWithRed:(83.0f32/255.0)
                                                                  green:(141.0f32/255.0)
                                                                   blue:(235.0f32/255.0)
                                                                  alpha:1.0f32],
                Part::MaximumTrack => msg_class![env; UIColor lightGrayColor],
                Part::Thumb => msg_class![env; UIColor whiteColor],
            }
        };
        () = msg![env; view setBackgroundColor:color];
        let radius: CGFloat = match (image != nil, part) {
            (true, _) => 0.0,
            (false, Part::Thumb) => DEFAULT_THUMB_SIZE / 2.0,
            (false, _) => DEFAULT_TRACK_HEIGHT / 2.0,
        };
        let layer: id = msg![env; view layer];
        () = msg![env; layer setCornerRadius:radius];
    }
    () = msg![env; this layoutSubviews];
}

fn part_size(env: &mut Environment, this: id, part: Part) -> CGSize {
    let image = image_for(env, this, part);
    if image != nil {
        return msg![env; image size];
    }
    match part {
        Part::Thumb => CGSize {
            width: DEFAULT_THUMB_SIZE,
            height: DEFAULT_THUMB_SIZE,
        },
        _ => CGSize {
            width: 0.0,
            height: DEFAULT_TRACK_HEIGHT,
        },
    }
}

/// The value as a fraction of the way from the minimum to the maximum.
fn fraction(host_object: &UISliderHostObject) -> f32 {
    let range = host_object.maximum_value - host_object.minimum_value;
    if range > 0.0 {
        (host_object.value - host_object.minimum_value) / range
    } else {
        0.0
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UISlider: UIControl

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UISliderHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    init_common(env, this)
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let this = init_common(env, this);
    for (key, setter) in [
        ("UIMinValue", "setMinimumValue:"),
        ("UIMaxValue", "setMaximumValue:"),
        ("UIValue", "setValue:"),
    ] {
        let key = get_static_str(env, key);
        if msg![env; coder containsValueForKey:key] {
            let value: f32 = msg![env; coder decodeFloatForKey:key];
            let sel = env.objc.lookup_selector(setter).unwrap();
            () = crate::objc::msg_send(env, (this, sel, value));
        }
    }
    this
}

- (())dealloc {
    let host_object = std::mem::take(env.objc.borrow_mut::<UISliderHostObject>(this));
    for view in [host_object.minimum_track, host_object.maximum_track, host_object.thumb] {
        release(env, view);
    }
    for (_, _, image) in host_object.images {
        release(env, image);
    }
    msg_super![env; this dealloc]
}

- (())layoutSubviews {
    let bounds: CGRect = msg![env; this bounds];
    let thumb_size = part_size(env, this, Part::Thumb);
    let min_track_height = part_size(env, this, Part::MinimumTrack).height;
    let max_track_height = part_size(env, this, Part::MaximumTrack).height;
    let host_object = env.objc.borrow::<UISliderHostObject>(this);
    let fraction = fraction(host_object);
    let &UISliderHostObject {
        minimum_track,
        maximum_track,
        thumb,
        ..
    } = host_object;

    let mid_y = bounds.origin.y + bounds.size.height / 2.0;
    let travel = (bounds.size.width - thumb_size.width).max(0.0);
    let thumb_x = bounds.origin.x + travel * fraction;
    let thumb_mid_x = thumb_x + thumb_size.width / 2.0;
    let track = |x: CGFloat, width: CGFloat, height: CGFloat| CGRect {
        origin: CGPoint { x, y: mid_y - height / 2.0 },
        size: CGSize { width, height },
    };
    let min_rect = track(bounds.origin.x, thumb_mid_x - bounds.origin.x, min_track_height);
    let max_width = bounds.origin.x + bounds.size.width - thumb_mid_x;
    let max_rect = track(thumb_mid_x, max_width, max_track_height);
    let thumb_rect = CGRect {
        origin: CGPoint { x: thumb_x, y: mid_y - thumb_size.height / 2.0 },
        size: thumb_size,
    };
    () = msg![env; minimum_track setFrame:min_rect];
    () = msg![env; maximum_track setFrame:max_rect];
    () = msg![env; thumb setFrame:thumb_rect];
}

- (id)hitTest:(CGPoint)point
    withEvent:(id)event { // UIEvent* (possibly nil)
    // The parts aren't hit separately: touches go to the slider itself.
    if msg![env; this pointInside:point withEvent:event] {
        this
    } else {
        nil
    }
}
- (bool)pointInside:(CGPoint)point
          withEvent:(id)_event { // UIEvent* (possibly nil)
    // The thumb is often taller than the slider.
    let bounds: CGRect = msg![env; this bounds];
    let thumb = env.objc.borrow::<UISliderHostObject>(this).thumb;
    let thumb_frame: CGRect = msg![env; thumb frame];
    let inside = |r: CGRect| {
        point.x >= r.origin.x
            && point.x < r.origin.x + r.size.width
            && point.y >= r.origin.y
            && point.y < r.origin.y + r.size.height
    };
    inside(bounds) || inside(thumb_frame)
}

// UIControl tracking: the thumb is dragged, the track isn't tapped.
- (bool)beginTrackingWithTouch:(id)touch // UITouch*
                     withEvent:(id)_event { // UIEvent*
    let point: CGPoint = msg![env; touch locationInView:this];
    let thumb = env.objc.borrow::<UISliderHostObject>(this).thumb;
    let frame: CGRect = msg![env; thumb frame];
    // A little slack around the thumb, as it's small for a finger.
    let slack = 10.0;
    if point.x < frame.origin.x - slack
        || point.x > frame.origin.x + frame.size.width + slack
    {
        return false;
    }
    let host_object = env.objc.borrow_mut::<UISliderHostObject>(this);
    host_object.grab_offset = point.x - (frame.origin.x + frame.size.width / 2.0);
    host_object.value_at_grab = host_object.value;
    true
}
- (bool)continueTrackingWithTouch:(id)touch // UITouch*
                        withEvent:(id)event { // UIEvent*
    let point: CGPoint = msg![env; touch locationInView:this];
    let bounds: CGRect = msg![env; this bounds];
    let thumb_width = part_size(env, this, Part::Thumb).width;
    let travel = bounds.size.width - thumb_width;
    let host_object = env.objc.borrow::<UISliderHostObject>(this);
    if travel <= 0.0 {
        return true;
    }
    let thumb_mid_x = point.x - host_object.grab_offset;
    let fraction = ((thumb_mid_x - bounds.origin.x - thumb_width / 2.0) / travel).clamp(0.0, 1.0);
    let range = host_object.maximum_value - host_object.minimum_value;
    let value = host_object.minimum_value + range * fraction;
    let (old_value, continuous) = (host_object.value, host_object.continuous);
    () = msg![env; this setValue:value];
    let new_value = env.objc.borrow::<UISliderHostObject>(this).value;
    if continuous && new_value != old_value {
        send_actions(env, this, event, UIControlEventValueChanged);
    }
    true
}
- (())endTrackingWithTouch:(id)touch // UITouch*
                 withEvent:(id)event { // UIEvent*
    () = msg_super![env; this endTrackingWithTouch:touch withEvent:event];
    let host_object = env.objc.borrow::<UISliderHostObject>(this);
    if !host_object.continuous && host_object.value != host_object.value_at_grab {
        send_actions(env, this, event, UIControlEventValueChanged);
    }
}

- (bool)isContinuous {
    env.objc.borrow::<UISliderHostObject>(this).continuous
}
- (())setContinuous:(bool)continuous {
    env.objc.borrow_mut::<UISliderHostObject>(this).continuous = continuous;
}

- (())setMinimumValueImage:(id)img { // UIImage *
    todo_objc_setter!(this, img);
}
- (())setMaximumValueImage:(id)img { // UIImage *
    todo_objc_setter!(this, img);
}

- (f32)minimumValue {
    env.objc.borrow::<UISliderHostObject>(this).minimum_value
}
- (())setMinimumValue:(f32)value {
    env.objc.borrow_mut::<UISliderHostObject>(this).minimum_value = value;
    let current: f32 = msg![env; this value];
    () = msg![env; this setValue:current];
}

- (f32)maximumValue {
    env.objc.borrow::<UISliderHostObject>(this).maximum_value
}
- (())setMaximumValue:(f32)value {
    env.objc.borrow_mut::<UISliderHostObject>(this).maximum_value = value;
    let current: f32 = msg![env; this value];
    () = msg![env; this setValue:current];
}

- (f32)value {
    env.objc.borrow::<UISliderHostObject>(this).value
}
- (())setValue:(f32)value {
    let host_object = env.objc.borrow_mut::<UISliderHostObject>(this);
    // Like Apple's, keep it between the minimum and maximum.
    host_object.value = value
        .min(host_object.maximum_value)
        .max(host_object.minimum_value);
    () = msg![env; this layoutSubviews];
}

- (())setValue:(f32)value animated:(bool)_animated {
    () = msg![env; this setValue:value];
}

- (())setThumbImage:(id)image
          forState:(UIControlState)state {
    set_image(env, this, Part::Thumb, state, image);
}
- (())setMinimumTrackImage:(id)image
                 forState:(UIControlState)state {
    set_image(env, this, Part::MinimumTrack, state, image);
}
- (())setMaximumTrackImage:(id)image
                 forState:(UIControlState)state {
    set_image(env, this, Part::MaximumTrack, state, image);
}

- (id)currentThumbImage {
    image_for(env, this, Part::Thumb)
}
- (id)currentMinimumTrackImage {
    image_for(env, this, Part::MinimumTrack)
}
- (id)currentMaximumTrackImage {
    image_for(env, this, Part::MaximumTrack)
}

- (())setHighlighted:(bool)highlighted {
    () = msg_super![env; this setHighlighted:highlighted];
    update(env, this);
}

@end

};
