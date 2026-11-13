/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIActivityIndicatorView`.

use crate::frameworks::core_graphics::cg_context::{
    CGContextAddLineToPoint, CGContextBeginPath, CGContextMoveToPoint, CGContextSetLineCap,
    CGContextSetLineWidth, CGContextSetRGBStrokeColor, CGContextStrokePath,
};
use crate::frameworks::core_graphics::cg_path::kCGLineCapRound;
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_super, objc_classes, ClassExports, NSZonePtr,
};
use crate::Environment;
use std::time::Instant;

type UIActivityIndicatorViewStyle = NSInteger;
const UIActivityIndicatorViewStyleWhiteLarge: UIActivityIndicatorViewStyle = 0;
const UIActivityIndicatorViewStyleWhite: UIActivityIndicatorViewStyle = 1;
const UIActivityIndicatorViewStyleGray: UIActivityIndicatorViewStyle = 2;

/// The spokes, one of which is brightest at a time, going round once a
/// second.
const SPOKES: u32 = 12;

pub struct UIActivityIndicatorViewHostObject {
    superclass: super::ui_view::UIViewHostObject,
    style: UIActivityIndicatorViewStyle,
    hides_when_stopped: bool,
    /// When it started animating, if it is.
    started: Option<Instant>,
}
impl_HostObject_with_superclass!(UIActivityIndicatorViewHostObject);

fn style_size(style: UIActivityIndicatorViewStyle) -> CGFloat {
    if style == UIActivityIndicatorViewStyleWhiteLarge {
        37.0
    } else {
        20.0
    }
}

/// Resizes the view for its style, keeping its center.
fn resize(env: &mut Environment, this: id) {
    let style = env
        .objc
        .borrow::<UIActivityIndicatorViewHostObject>(this)
        .style;
    let size = style_size(style);
    let mut bounds: CGRect = msg![env; this bounds];
    bounds.size = CGSize {
        width: size,
        height: size,
    };
    () = msg![env; this setBounds:bounds];
    () = msg![env; this setNeedsDisplay];
}

fn update_hidden(env: &mut Environment, this: id) {
    let host_obj = env.objc.borrow::<UIActivityIndicatorViewHostObject>(this);
    if host_obj.hides_when_stopped {
        let hidden = host_obj.started.is_none();
        () = msg![env; this setHidden:hidden];
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIActivityIndicatorView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIActivityIndicatorViewHostObject {
        superclass: Default::default(),
        style: UIActivityIndicatorViewStyleWhite,
        hides_when_stopped: true,
        started: None,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    () = msg![env; this setOpaque:false];
    update_hidden(env, this);
    this
}

- (id)initWithActivityIndicatorStyle:(UIActivityIndicatorViewStyle)style {
    let size = style_size(style);
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: size,
            height: size,
        },
    };
    let this: id = msg![env; this initWithFrame:frame];
    env.objc
        .borrow_mut::<UIActivityIndicatorViewHostObject>(this)
        .style = style;
    this
}

- (UIActivityIndicatorViewStyle)activityIndicatorViewStyle {
    env.objc.borrow::<UIActivityIndicatorViewHostObject>(this).style
}
- (())setActivityIndicatorViewStyle:(UIActivityIndicatorViewStyle)style {
    env.objc
        .borrow_mut::<UIActivityIndicatorViewHostObject>(this)
        .style = style;
    resize(env, this);
}

- (())startAnimating {
    let host_obj = env.objc.borrow_mut::<UIActivityIndicatorViewHostObject>(this);
    if host_obj.started.is_none() {
        host_obj.started = Some(Instant::now());
        update_hidden(env, this);
        () = msg![env; this setNeedsDisplay];
    }
}
- (())stopAnimating {
    env.objc
        .borrow_mut::<UIActivityIndicatorViewHostObject>(this)
        .started = None;
    update_hidden(env, this);
    () = msg![env; this setNeedsDisplay];
}
- (bool)isAnimating {
    env.objc
        .borrow::<UIActivityIndicatorViewHostObject>(this)
        .started
        .is_some()
}

- (bool)hidesWhenStopped {
    env.objc
        .borrow::<UIActivityIndicatorViewHostObject>(this)
        .hides_when_stopped
}
- (())setHidesWhenStopped:(bool)hides {
    env.objc
        .borrow_mut::<UIActivityIndicatorViewHostObject>(this)
        .hides_when_stopped = hides;
    update_hidden(env, this);
}

- (())drawRect:(CGRect)_rect {
    let bounds: CGRect = msg![env; this bounds];
    let host_obj = env.objc.borrow::<UIActivityIndicatorViewHostObject>(this);
    let (style, started) = (host_obj.style, host_obj.started);
    // Stepped by time, not a timer: on iOS it turns even while the game
    // keeps the main thread busy (e.g. loading).
    let step = started.map_or(0, |started| {
        (started.elapsed().as_secs_f64() * f64::from(SPOKES)) as u32 % SPOKES
    });
    let shade = if style == UIActivityIndicatorViewStyleGray {
        0.5
    } else {
        1.0
    };
    let radius = bounds.size.width.min(bounds.size.height) / 2.0;
    let center = CGPoint {
        x: bounds.origin.x + bounds.size.width / 2.0,
        y: bounds.origin.y + bounds.size.height / 2.0,
    };
    let width = radius * 0.2;
    // Round caps reach past the ends by half the width.
    let (inner, outer) = (radius * 0.5, radius - width / 2.0);

    let context = UIGraphicsGetCurrentContext(env);
    CGContextSetLineWidth(env, context, width);
    CGContextSetLineCap(env, context, kCGLineCapRound);
    for i in 0..SPOKES {
        // Clockwise from the top; the spokes behind the brightest fade.
        let angle = (i as CGFloat / SPOKES as CGFloat - 0.25) * std::f32::consts::TAU;
        let behind = (step + SPOKES - i) % SPOKES;
        let alpha = 1.0 - 0.75 * behind as CGFloat / (SPOKES - 1) as CGFloat;
        CGContextSetRGBStrokeColor(env, context, shade, shade, shade, alpha);
        let (sin, cos) = angle.sin_cos();
        CGContextBeginPath(env, context);
        CGContextMoveToPoint(env, context, center.x + cos * inner, center.y + sin * inner);
        CGContextAddLineToPoint(env, context, center.x + cos * outer, center.y + sin * outer);
        CGContextStrokePath(env, context);
    }
    if started.is_some() {
        () = msg![env; this setNeedsDisplay];
    }
}

@end

};
