/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CALayer`.

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::core_animation::ca_animation::get_transition;
use crate::frameworks::core_animation::ca_transaction;
use crate::frameworks::core_animation::ca_transform_3d::CATransform3D;
use crate::frameworks::core_foundation::time::CFTimeInterval;
use crate::frameworks::core_graphics::cg_affine_transform::{
    CGAffineTransform, CGAffineTransformIdentity,
};
use crate::frameworks::core_graphics::cg_bitmap_context::{
    CGBitmapContextCreate, CGBitmapContextGetHeight, CGBitmapContextGetWidth,
};
use crate::frameworks::core_graphics::cg_color::{CGColorHostObject, CGColorRef};
use crate::frameworks::core_graphics::cg_color_space::CGColorSpaceCreateDeviceRGB;
use crate::frameworks::core_graphics::cg_context::{
    set_ctm, CGContextClearRect, CGContextGetCTM, CGContextRef, CGContextRelease,
    CGContextScaleCTM, CGContextTranslateCTM,
};
use crate::frameworks::core_graphics::cg_image::{
    kCGImageAlphaPremultipliedLast, kCGImageByteOrder32Big,
};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{self, get_static_str, to_rust_string};
use crate::mem::{GuestUSize, Ptr};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, todo_objc_setter,
    ClassExports, HostObject, ObjC,
};
use crate::Environment;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) struct CALayerHostObject {
    /// Possibly nil, usually a UIView. This is a weak reference.
    delegate: id,
    /// Sublayers in back-to-front order. These are strong references.
    pub(super) sublayers: Vec<id>,
    /// The superlayer. This is a weak reference.
    superlayer: id,
    pub(super) bounds: CGRect,
    pub(super) position: CGPoint,
    pub(super) anchor_point: CGPoint,
    pub(super) affine_transform: CGAffineTransform,
    pub(super) hidden: bool,
    pub(super) opaque: bool,
    pub(super) opacity: f32,
    pub(super) z_position: CGFloat,
    pub(super) background_color: Option<CGColorHostObject>,
    pub(super) corner_radius: CGFloat,
    pub(super) needs_display: bool,
    pub(super) needs_display_on_bounds_change: bool,
    /// `CGImageRef*`
    pub(super) contents: id,
    /// The part of `contents` to show, in unit co-ordinates.
    pub(super) contents_rect: CGRect,
    /// For CAEAGLLayer only
    pub(super) drawable_properties: id,
    /// For CAEAGLLayer only (internal state for compositor)
    pub(super) presented_pixels: Option<(Vec<u8>, u32, u32)>,
    /// Internal, only exposed when calling `drawLayer:inContext:`
    pub(super) cg_context: Option<CGContextRef>,
    /// Internal state for compositor
    pub(super) gles_texture: Option<crate::gles::gles11_raw::types::GLuint>,
    /// Internal state for compositor
    pub(super) gles_texture_is_up_to_date: bool,
    /// Internal state for compositor: the `contents` the texture was made
    /// from, which an animation can change without `setContents:`.
    pub(super) gles_texture_contents: id,
    pub(super) animations: HashMap<String, id>, // CAAnimation*
    pub(super) anonymous_animations: HashSet<id>, // CAAnimation*
    /// Presentation layer only: a fade transition and how far it's gone (0
    /// to 1), for the compositor.
    pub(super) fade_transition: Option<(id, f32)>,
    /// Where `contents` goes in the bounds.
    pub(super) contents_gravity: ContentsGravity,
    /// Whether the contents and sublayers are clipped to the bounds.
    pub(super) masks_to_bounds: bool,
}

/// Where a layer's `contents` go in its bounds. This is set from a view's
/// `contentMode`, so `Top` and so on mean the top as seen on screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContentsGravity {
    Resize,
    ResizeAspect,
    ResizeAspectFill,
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ContentsGravity {
    /// Where contents of `size` go in `bounds`. They aren't clipped.
    pub(super) fn rect(self, bounds: CGRect, size: CGSize) -> CGRect {
        use ContentsGravity::*;
        let size = match self {
            Resize => return bounds,
            ResizeAspect | ResizeAspectFill => {
                let (sx, sy) = (
                    bounds.size.width / size.width,
                    bounds.size.height / size.height,
                );
                let scale = if self == ResizeAspect {
                    sx.min(sy)
                } else {
                    sx.max(sy)
                };
                CGSize {
                    width: size.width * scale,
                    height: size.height * scale,
                }
            }
            _ => size,
        };
        // How far across and down: 0 is left or top, 1 is right or bottom.
        let (fx, fy) = match self {
            Top => (0.5, 0.0),
            Bottom => (0.5, 1.0),
            Left => (0.0, 0.5),
            Right => (1.0, 0.5),
            TopLeft => (0.0, 0.0),
            TopRight => (1.0, 0.0),
            BottomLeft => (0.0, 1.0),
            BottomRight => (1.0, 1.0),
            _ => (0.5, 0.5),
        };
        CGRect {
            origin: CGPoint {
                x: bounds.origin.x + ((bounds.size.width - size.width) * fx).round(),
                y: bounds.origin.y + ((bounds.size.height - size.height) * fy).round(),
            },
            size,
        }
    }
}

/// For `UIView`'s `contentMode`.
pub fn set_contents_gravity(env: &mut Environment, layer: id, gravity: ContentsGravity) {
    env.objc
        .borrow_mut::<CALayerHostObject>(layer)
        .contents_gravity = gravity;
}
impl HostObject for CALayerHostObject {}

impl CALayerHostObject {
    /// Internal helper method: generate a transformation matrix to transform
    /// from the superlayer's co-ordinate space (the space that the layer's
    /// position is specified in) to the layer's internal co-ordinate space
    /// (the space that the layer's bounds and its sublayers' positions are
    /// specified in).
    pub(super) fn superlayer_to_layer_transform(&self) -> CGAffineTransform {
        CGAffineTransform::make_translation(-self.bounds.origin.x, -self.bounds.origin.y)
            .concat(CGAffineTransform::make_translation(
                -self.bounds.size.width * self.anchor_point.x,
                -self.bounds.size.height * self.anchor_point.y,
            ))
            .concat(self.affine_transform)
            .concat(CGAffineTransform::make_translation(
                self.position.x,
                self.position.y,
            ))
    }
}

pub const kCAFilterLinear: &str = "kCAFilterLinear";
pub const kCAFilterNearest: &str = "kCAFilterNearest";
pub const kCAFilterTrilinear: &str = "kCAFilterTrilinear";

pub const CONSTANTS: ConstantExports = &[
    ("_kCAFilterLinear", HostConstant::NSString(kCAFilterLinear)),
    (
        "_kCAFilterNearest",
        HostConstant::NSString(kCAFilterNearest),
    ),
    (
        "_kCAFilterTrilinear",
        HostConstant::NSString(kCAFilterTrilinear),
    ),
];

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation CALayer: NSObject

+ (id)alloc {
    let host_object = Box::new(CALayerHostObject {
        delegate: nil,
        sublayers: Vec::new(),
        superlayer: nil,
        bounds: CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize { width: 0.0, height: 0.0 }
        },
        position: CGPoint { x: 0.0, y: 0.0 },
        anchor_point: CGPoint { x: 0.5, y: 0.5 },
        affine_transform: CGAffineTransformIdentity,
        hidden: false,
        opaque: false,
        opacity: 1.0,
        z_position: 0.0,
        background_color: None, // transparency
        corner_radius: 0.0,
        needs_display: false,
        needs_display_on_bounds_change: false,
        contents: nil,
        contents_rect: CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize { width: 1.0, height: 1.0 }
        },
        drawable_properties: nil,
        presented_pixels: None,
        cg_context: None,
        gles_texture: None,
        gles_texture_is_up_to_date: false,
        gles_texture_contents: nil,
        animations: HashMap::new(),
        anonymous_animations: HashSet::new(),
        fade_transition: None,
        contents_gravity: ContentsGravity::Resize,
        masks_to_bounds: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)layer {
    let new_layer: id = msg![env; this alloc];
    msg![env; new_layer init]
}

- (())dealloc {
    let &mut CALayerHostObject {
        drawable_properties,
        contents,
        superlayer,
        cg_context,
        ref mut sublayers,
        ..
    } = env.objc.borrow_mut(this);
    let sublayers = std::mem::take(sublayers);

    if drawable_properties != nil {
        release(env, drawable_properties);
    }

    if contents != nil {
        release(env, contents);
    }

    if let Some(cg_context) = cg_context {
        CGContextRelease(env, cg_context);
    }

    assert!(superlayer == nil);
    for sublayer in sublayers {
        env.objc.borrow_mut::<CALayerHostObject>(sublayer).superlayer = nil;
        release(env, sublayer);
    }

    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)delegate {
    env.objc.borrow::<CALayerHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<CALayerHostObject>(this).delegate = delegate;
}

- (id)superlayer {
    env.objc.borrow::<CALayerHostObject>(this).superlayer
}
// TODO: sublayers accessors

- (())addSublayer:(id)layer {
    if env.objc.borrow::<CALayerHostObject>(layer).superlayer == this {
        () = msg![env; this bringSublayerToFront:layer];
    } else {
        retain(env, layer);
        () = msg![env; layer removeFromSuperlayer];
        env.objc.borrow_mut::<CALayerHostObject>(layer).superlayer = this;
        env.objc.borrow_mut::<CALayerHostObject>(this).sublayers.push(layer);
    }
}

- (())insertSublayer:(id)layer atIndex:(u32)idx {
    retain(env, layer);
    () = msg![env; layer removeFromSuperlayer];
    env.objc.borrow_mut::<CALayerHostObject>(layer).superlayer = this;

    let CALayerHostObject { ref mut sublayers, .. } = env.objc.borrow_mut(this);
    sublayers.insert(idx.try_into().unwrap(), layer);
}

- (())insertSublayer:(id)layer below:(id)sibling {
    retain(env, layer);
    () = msg![env; layer removeFromSuperlayer];
    env.objc.borrow_mut::<CALayerHostObject>(layer).superlayer = this;

    let CALayerHostObject { ref mut sublayers, .. } = env.objc.borrow_mut(this);
    let idx = sublayers.iter().position(|&sublayer| sublayer == sibling).unwrap();
    sublayers.insert(idx, layer);
}

- (())removeFromSuperlayer {
    let CALayerHostObject { ref mut superlayer, .. } = env.objc.borrow_mut(this);
    let superlayer = std::mem::take(superlayer);
    if superlayer == nil {
        return;
    }

    let CALayerHostObject { ref mut sublayers, .. } = env.objc.borrow_mut(superlayer);
    let idx = sublayers.iter().position(|&sublayer| sublayer == this).unwrap();
    let sublayer = sublayers.remove(idx);
    assert!(sublayer == this);
    release(env, this);
}

- (CGRect)bounds {
    env.objc.borrow::<CALayerHostObject>(this).bounds
}
- (())setBounds:(CGRect)bounds {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_bounds = std::mem::replace(&mut host_object.bounds, bounds);
    if is_implicit_animation_enabled(env, this) && old_bounds != bounds {
        let old_bounds: id = msg_class![env; NSValue valueWithCGRect:old_bounds];
        let bounds: id = msg_class![env; NSValue valueWithCGRect:bounds];
        add_default_implied_basic_animation(env, this, "bounds", old_bounds, bounds);
    }
    if env.objc.borrow::<CALayerHostObject>(this).needs_display_on_bounds_change {
        () = msg![env; this setNeedsDisplay];
    }
}

- (CGPoint)position {
    env.objc.borrow::<CALayerHostObject>(this).position
}
- (())setPosition:(CGPoint)position {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_position = std::mem::replace(&mut host_object.position, position);
    if is_implicit_animation_enabled(env, this) && old_position != position {
        let old_position: id = msg_class![env; NSValue valueWithCGPoint:old_position];
        let position: id = msg_class![env; NSValue valueWithCGPoint:position];
        add_default_implied_basic_animation(env, this, "position", old_position, position);
    }
}

- (CGPoint)anchorPoint {
    env.objc.borrow::<CALayerHostObject>(this).anchor_point
}
- (())setAnchorPoint:(CGPoint)anchor_point {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_anchor_point = std::mem::replace(&mut host_object.anchor_point, anchor_point);
    if is_implicit_animation_enabled(env, this) && old_anchor_point != anchor_point {
        let old_anchor_point: id = msg_class![env; NSValue valueWithCGPoint:old_anchor_point];
        let anchor_point: id = msg_class![env; NSValue valueWithCGPoint:anchor_point];
        add_default_implied_basic_animation(env, this, "anchorPoint", old_anchor_point, anchor_point);
    }
}

- (CGAffineTransform)affineTransform {
    env.objc.borrow::<CALayerHostObject>(this).affine_transform
}
- (CATransform3D)transform {
    env.objc.borrow::<CALayerHostObject>(this).affine_transform.into()
}
- (())setTransform:(CATransform3D)transform {
    if !transform.is_affine() {
        log_once!("Warning: only the 2D part of 3D layer transforms is supported.");
    }
    () = msg![env; this setAffineTransform:(transform.affine_part())];
}
- (())setAffineTransform:(CGAffineTransform)affine_transform {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_affine_transform = std::mem::replace(&mut host_object.affine_transform, affine_transform);
    if is_implicit_animation_enabled(env, this) && old_affine_transform != affine_transform {
        log!("TODO: Implicit animation for affineTransform change from {old_affine_transform:?} to {affine_transform:?}");
    }
}

- (CGRect)frame {
    let host_obj @ &CALayerHostObject {
        bounds,
        ..
    } = env.objc.borrow(this);
    host_obj.superlayer_to_layer_transform().apply_to_rect(CGRect {
        origin: CGPoint { x: bounds.origin.x, y: bounds.origin.y },
        size: bounds.size,
    })
}
- (())setFrame:(CGRect)frame {
    let CALayerHostObject {
        anchor_point,
        affine_transform,
        ..
    } = env.objc.borrow_mut(this);

    let inverse_transform = CGAffineTransform::make_translation(
        -frame.size.width * anchor_point.x,
        -frame.size.height * anchor_point.y,
    )
    .concat(*affine_transform).invert();

    // Not the same as ::apply_to_size() as this does not ignore translation.
    let transformed_size = inverse_transform.apply_to_rect(CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: frame.size
    }).size;
    let transformed_offset = inverse_transform.apply_to_point(CGPoint { x: 0.0, y: 0.0 });

    let new_position = CGPoint {
        x: frame.origin.x + transformed_offset.x,
        y: frame.origin.y + transformed_offset.y,
    };
    () = msg![env; this setPosition:new_position];
    let new_bounds = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: transformed_size,
    };
    () = msg![env; this setBounds:new_bounds];
}

- (bool)isHidden {
    env.objc.borrow::<CALayerHostObject>(this).hidden
}
- (())setHidden:(bool)hidden {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_hidden = std::mem::replace(&mut host_object.hidden, hidden);
    if is_implicit_animation_enabled(env, this) && old_hidden != hidden {
        // i kinda hate this
        let old_hidden: id = msg_class![env; NSNumber numberWithBool:old_hidden];
        let hidden: id = msg_class![env; NSNumber numberWithBool:hidden];
        add_default_implied_basic_animation(env, this, "hidden", old_hidden, hidden);
    }
}

- (bool)masksToBounds {
    env.objc.borrow::<CALayerHostObject>(this).masks_to_bounds
}
- (())setMasksToBounds:(bool)masks {
    env.objc.borrow_mut::<CALayerHostObject>(this).masks_to_bounds = masks;
}

- (bool)isOpaque {
    env.objc.borrow::<CALayerHostObject>(this).opaque
}
- (())setOpaque:(bool)opaque {
    env.objc.borrow_mut::<CALayerHostObject>(this).opaque = opaque;
}

- (CGFloat)zPosition {
    env.objc.borrow::<CALayerHostObject>(this).z_position
}
- (())setZPosition:(CGFloat)z_position {
    env.objc.borrow_mut::<CALayerHostObject>(this).z_position = z_position;
}

- (f32)opacity {
    env.objc.borrow::<CALayerHostObject>(this).opacity
}
- (())setOpacity:(f32)opacity {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_opacity = std::mem::replace(&mut host_object.opacity, opacity);
    if is_implicit_animation_enabled(env, this) && old_opacity != opacity {
        let old_opacity: id = msg_class![env; NSNumber numberWithFloat:old_opacity];
        let opacity: id = msg_class![env; NSNumber numberWithFloat:opacity];
        add_default_implied_basic_animation(env, this, "opacity", old_opacity, opacity);
    }
}

- (CGColorRef)backgroundColor {
    if let Some(bg_color) = env.objc.borrow::<CALayerHostObject>(this).background_color {
        let class = env.objc.get_known_class("_touchHLE_CGColor", &mut env.mem);
        let obj = env.objc.alloc_object(class, Box::new(bg_color), &mut env.mem);
        autorelease(env, obj)
    } else {
        nil
    }
}
- (())setBackgroundColor:(CGColorRef)new_color_ref {
    let old_color_ref = msg![env; this backgroundColor];
    let new_color = if new_color_ref == nil {
        None
    } else {
        Some(*env.objc.borrow::<CGColorHostObject>(new_color_ref))
    };
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    host_object.background_color = new_color;
    if is_implicit_animation_enabled(env, this) && old_color_ref != nil && new_color_ref != nil {
        add_default_implied_basic_animation(env, this, "backgroundColor", old_color_ref, new_color_ref);
    }
}

- (CGFloat)cornerRadius {
    env.objc.borrow::<CALayerHostObject>(this).corner_radius
}
- (())setCornerRadius:(CGFloat)corner_radius {
    let host_object = env.objc.borrow_mut::<CALayerHostObject>(this);
    let old_corner_radius = std::mem::replace(&mut host_object.corner_radius, corner_radius);
    if is_implicit_animation_enabled(env, this) && old_corner_radius != corner_radius {
        let old_corner_radius: id = msg_class![env; NSNumber numberWithFloat:old_corner_radius];
        let corner_radius: id = msg_class![env; NSNumber numberWithFloat:corner_radius];
        add_default_implied_basic_animation(env, this, "cornerRadius", old_corner_radius, corner_radius);
    }
}

- (bool)needsDisplay {
    env.objc.borrow::<CALayerHostObject>(this).needs_display
}
- (())setNeedsDisplay {
    env.objc.borrow_mut::<CALayerHostObject>(this).needs_display = true;
}

- (bool)needsDisplayOnBoundsChange {
    env.objc.borrow::<CALayerHostObject>(this).needs_display_on_bounds_change
}
- (())setNeedsDisplayOnBoundsChange:(bool)value {
    env.objc.borrow_mut::<CALayerHostObject>(this).needs_display_on_bounds_change = value;
}

// TODO: support setNeedsDisplayInRect:
- (())displayIfNeeded {
    let &mut CALayerHostObject {
        ref mut needs_display,
        delegate,
        ..
    } = env.objc.borrow_mut(this);
    if !std::mem::take(needs_display) {
        return;
    }

    if delegate == nil {
        return;
    }

    let delegate_class = ObjC::read_isa(delegate, &env.mem);

    // According to the Core Animation Programming Guide, a layer delegate must
    // provide either displayLayer: or drawLayer:inContext:, and the former is
    // called if both are defined.

    if env.objc.class_has_method_named(delegate_class, "displayLayer:") {
        () = msg![env; delegate displayLayer:this];
        return;
    }

    let &mut CALayerHostObject {
        cg_context,
        ref mut gles_texture_is_up_to_date,
        bounds: CGRect { origin, size },
        ..
    } = env.objc.borrow_mut(this);

    *gles_texture_is_up_to_date = false;

    // The app picker draws at the screen's real resolution, so its text is
    // sharp. For apps this is 1.
    let scale = env.window.as_ref().map_or(1.0, |window| window.ui_scale());

    // TODO: more correctly handle non-integer sizes?
    let int_width = (size.width * scale).round() as GuestUSize;
    let int_height = (size.height * scale).round() as GuestUSize;

    let need_new_context = cg_context.is_none_or(|existing|
            CGBitmapContextGetWidth(env, existing) != int_width ||
            CGBitmapContextGetHeight(env, existing) != int_height
    );
    let cg_context = if need_new_context {
        if let Some(old_context) = cg_context {
            CGContextRelease(env, old_context);
        }

        // Make sure this is in sync with the code in composition.rs that
        // uploads the texture!
        // TODO: is this the right color space?
        let color_space = CGColorSpaceCreateDeviceRGB(env);
        let cg_context = CGBitmapContextCreate(
            env,
            Ptr::null(),
            int_width,
            int_height,
            8, // bpp
            int_width.checked_mul(4).unwrap(),
            color_space,
            kCGImageByteOrder32Big | kCGImageAlphaPremultipliedLast
        );
        env.objc.borrow_mut::<CALayerHostObject>(this).cg_context = Some(cg_context);
        cg_context
    } else {
        cg_context.unwrap()
    };

    let ctm = CGContextGetCTM(env, cg_context);
    CGContextScaleCTM(env, cg_context, scale, scale);
    CGContextTranslateCTM(env, cg_context, -origin.x, -origin.y);
    // TODO: move clearing to UIKit (clearsContextBeforeDrawing)?
    CGContextClearRect(env, cg_context, CGRect { origin, size });
    () = msg![env; delegate drawLayer:this inContext:cg_context];
    set_ctm(env, cg_context, ctm);
}

// CGImageRef*
- (id)sublayers {
    let sublayers = env.objc.borrow::<CALayerHostObject>(this).sublayers.clone();
    if sublayers.is_empty() {
        return nil;
    }
    for &sublayer in &sublayers {
        retain(env, sublayer);
    }
    let array = crate::frameworks::foundation::ns_array::from_vec(env, sublayers);
    autorelease(env, array)
}

- (id)contents {
    env.objc.borrow::<CALayerHostObject>(this).contents
}
- (())setContents:(id)new_contents {
    let host_obj = env.objc.borrow_mut::<CALayerHostObject>(this);
    host_obj.gles_texture_is_up_to_date = false;
    let old_contents = std::mem::replace(&mut host_obj.contents, new_contents);
    retain(env, new_contents);
    release(env, old_contents);
}

- (CGRect)contentsRect {
    env.objc.borrow::<CALayerHostObject>(this).contents_rect
}
- (())setContentsRect:(CGRect)rect {
    // TODO: implicit animation
    env.objc.borrow_mut::<CALayerHostObject>(this).contents_rect = rect;
}

- (())setEdgeAntialiasingMask:(u32)mask {
    todo_objc_setter!(this, mask);
}

- (())setMagnificationFilter:(id)filter {
    todo_objc_setter!(this, ns_string::to_rust_string(env, filter));
}

- (())setMinificationFilter:(id)filter {
    todo_objc_setter!(this, ns_string::to_rust_string(env, filter));
}

- (bool)containsPoint:(CGPoint)point {
    let bounds: CGRect = msg![env; this bounds];
    let x_range = bounds.origin.x..(bounds.origin.x + bounds.size.width);
    let y_range = bounds.origin.y..(bounds.origin.y + bounds.size.height);
    let CGPoint {x, y} = point;
    x_range.contains(&x) && y_range.contains(&y)
}

- (CGPoint)convertPoint:(CGPoint)point
              fromLayer:(id)other { // CALayer*

    if this == other {
        return point;
    }

    let res = transform_for_conversion(env, this, other).apply_to_point(point);
    log_dbg!("Converted {point:?} from {other:?} to {this:?}: {res:?}");
    res
}
- (CGPoint)convertPoint:(CGPoint)point
                toLayer:(id)other { // CALayer*
    if this == other {
        return point;
    }

    let res = transform_for_conversion(env, other, this).apply_to_point(point);
    log_dbg!("Converted {point:?} from {this:?} to {other:?}: {res:?}");
    res
}
- (CGRect)convertRect:(CGRect)rect
            fromLayer:(id)other { // CALayer*

    if this == other {
        return rect;
    }

    let res = transform_for_conversion(env, this, other).apply_to_rect(rect);
    log_dbg!("Converted {rect:?} from {other:?} to {this:?}: {res:?}");
    res
}
- (CGRect)convertRect:(CGRect)rect
              toLayer:(id)other { // CALayer*
    if this == other {
        return rect;
    }

    let res = transform_for_conversion(env, other, this).apply_to_rect(rect);
    log_dbg!("Converted {rect:?} from {this:?} to {other:?}: {res:?}");
    res
}

- (())addAnimation:(id)anim // CAAnimation*
            forKey:(id)key { // NSString*
    // Like Core Animation, keep a copy, so the app can reuse the original.
    let anim: id = msg![env; anim copy];
    if get_transition(env, anim).is_some() {
        // The compositor keeps a picture of the layer from before.
        super::composition::add_transition(env, this, anim);
    }
    let duration: CFTimeInterval = msg![env; anim duration];
    if duration == 0.0 {
        // From the docs:
        //  If the duration property of the animation is zero or negative, the
        //  duration is changed to the current value of the
        //  kCATransactionAnimationDuration transaction property (if set) or to
        //  the default value of 0.25 seconds.
        let duration: CFTimeInterval = msg_class![env; CATransaction animationDuration];
        () = msg![env; anim setDuration:duration];
    }

    if key == nil {
        log_dbg!("[(CALayer*){:?} addAnimation:{:?} forKey:{:?}]", this, anim, key);
        let inserted = env.objc.borrow_mut::<CALayerHostObject>(this).anonymous_animations.insert(anim);
        assert!(inserted);
    } else {
        let key_string = to_rust_string(env, key);
        log_dbg!("[(CALayer*){:?} addAnimation:{:?} forKey:{:?} ({:?})]", this, anim, key, key_string);
        let host_obj = env.objc.borrow_mut::<CALayerHostObject>(this);
        // An animation with the same key replaces the old one.
        if let Some(old) = host_obj.animations.insert(key_string.to_string(), anim) {
            release(env, old);
        }
    }
}

- (id)animationForKey:(id)key { // NSString*
    let key_string = to_rust_string(env, key);
    let host_obj = env.objc.borrow::<CALayerHostObject>(this);
    host_obj.animations.get(&*key_string).copied().unwrap_or(nil)
}

- (())removeAnimationForKey:(id)key { // NSString*
    let key_string = to_rust_string(env, key);
    log_dbg!("[(CALayer*){:?} removeAnimationForKey:{:?} ({:?})]", this, key, key_string);
    if let Some(anim) = env.objc.borrow_mut::<CALayerHostObject>(this).animations.remove(&*key_string) {
        release(env, anim);
    };
}

- (())removeAllAnimations {
    log_dbg!("[(CALayer*){:?} removeAllAnimations]", this);
    let host_obj = env.objc.borrow_mut::<CALayerHostObject>(this);
    let animations: Vec<id> = host_obj.animations.drain().map(|(_, anim)| anim)
        .chain(host_obj.anonymous_animations.drain())
        .collect();
    for anim in animations {
        release(env, anim);
    }
}

// TODO: more

@end

};

pub fn remove_anonymous_animation(env: &mut Environment, layer: id, animation: id) {
    let removed = env
        .objc
        .borrow_mut::<CALayerHostObject>(layer)
        .anonymous_animations
        .remove(&animation);
    // It may already be gone, e.g. after -removeAllAnimations.
    if removed {
        release(env, animation);
    }
}

fn transform_for_conversion(env: &mut Environment, this: id, other: id) -> CGAffineTransform {
    // The convertPoint methods can be used in two ways:
    // - If two layers are provided (one as the receiver, one as a parameter),
    //   then the layers are required to have a common ancestor, and it will be
    //   used to provide a reference for converting the point/rect.
    // - If one layer is provided, and the other layer is nil, then the layer
    //   is resolved to the co-ordinate space of the origin of the layer at the
    //   top of the hierarchy. This is effectively the same as screen space, or
    //   the co-ordinate space that windows live in.
    let need_common_ancestor = this != nil && other != nil;
    assert!(!(this == nil && other == nil));

    // This algorithm attempts to efficiently find the common ancestor of the
    // two layers by walking up each layer's superlayer chain, one at a time,
    // alternating between layers until it finds a match.
    // For the single-layer case, it of course only walks its superlayer chain.

    // Maps of layer pointers to transforms that map that layer's co-ordinate
    // space to that of the starting layer for the iteration.
    let mut this_map = HashMap::from([(this, CGAffineTransformIdentity)]);
    let mut other_map = HashMap::from([(other, CGAffineTransformIdentity)]);
    // Current iteration state.
    let mut this_superlayer = this;
    let mut this_transform = CGAffineTransformIdentity;
    let mut other_superlayer = other;
    let mut other_transform = CGAffineTransformIdentity;
    let (common_ancestor, this_transform, other_transform) = loop {
        if this_superlayer != nil {
            let this_hostobj: &CALayerHostObject = env.objc.borrow(this_superlayer);
            let next = this_hostobj.superlayer;
            let next_transform =
                this_transform.concat(this_hostobj.superlayer_to_layer_transform());
            if need_common_ancestor && next != nil {
                if let Some(&other_transform) = other_map.get(&next) {
                    break (next, next_transform, other_transform);
                }
                this_map.insert(next, next_transform);
            }
            this_superlayer = next;
            this_transform = next_transform;
        }

        if other_superlayer != nil {
            let other_hostobj: &CALayerHostObject = env.objc.borrow(other_superlayer);
            let next = other_hostobj.superlayer;
            let next_transform =
                other_transform.concat(other_hostobj.superlayer_to_layer_transform());
            if need_common_ancestor && next != nil {
                if let Some(&this_transform) = this_map.get(&next) {
                    break (next, this_transform, next_transform);
                }
                other_map.insert(next, next_transform);
            }
            other_superlayer = next;
            other_transform = next_transform;
        }

        if this_superlayer == nil && other_superlayer == nil {
            if need_common_ancestor {
                panic!("Layers {this:?} and {other:?} have no common ancestor!");
            } else {
                break (nil, this_transform, other_transform);
            }
        }
    };

    assert!((common_ancestor == nil) != need_common_ancestor);
    if need_common_ancestor {
        log_dbg!("{this:?} and {other:?}'s common ancestor: {common_ancestor:?}",);
    }
    log_dbg!("{this:?}'s transform in {common_ancestor:?}: {this_transform:?}");
    log_dbg!("{other:?}'s transform in {common_ancestor:?}: {other_transform:?}");
    let other_to_this = other_transform.concat(this_transform.invert());
    log_dbg!("Transform from {other:?} to {this:?}: {other_to_this:?}");
    other_to_this
}

fn add_default_implied_basic_animation(
    env: &mut Environment,
    layer: id,
    key_path: &'static str,
    from_value: id,
    to_value: id,
) {
    let key_path = get_static_str(env, key_path);
    let animation = msg_class![env; CABasicAnimation animationWithKeyPath:key_path];
    () = msg![env; animation setFromValue: from_value];
    () = msg![env; animation setToValue: to_value];
    ca_transaction::ThreadLocalState::add_animation(env, layer, animation);
}

// TODO: Remove once CAActions are implemented
fn is_implicit_animation_enabled(env: &mut Environment, layer: id) -> bool {
    // CALayers have implicit animations enabled by default, but UIKit doesn't
    // unless there's an active UIView animation block, and animations haven't
    // been turned off.
    if ca_transaction::ThreadLocalState::get_current_transaction(env)
        .is_some_and(|transaction| transaction.disables_actions())
    {
        return false;
    }
    let delegate = msg![env; layer delegate];
    let uiview_class = env.objc.get_known_class("UIView", &mut env.mem);
    let delegate_is_uiview: bool = msg![env; delegate isKindOfClass:uiview_class];
    let ui_view = &env.framework_state.uikit.ui_view;
    !delegate_is_uiview || (ui_view.animation_block_count > 0 && !ui_view.animations_disabled)
}
