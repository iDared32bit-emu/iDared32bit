/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CGGradient.h`

use super::cg_color_space::{kCGColorSpaceModelRGB, CGColorSpaceGetModel, CGColorSpaceRef};
use super::CGFloat;
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::{CFRelease, CFRetain, CFTypeRef};
use crate::mem::{ConstPtr, GuestUSize};
use crate::objc::{objc_classes, ClassExports, HostObject, ObjC};
use crate::Environment;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CGGradient is a CFType-based type, but in our implementation those are
// just Objective-C types, so we need a class for it, but its name is not
// visible anywhere.
@implementation _touchHLE_CGGradient: NSObject
@end

};

type Rgba = (CGFloat, CGFloat, CGFloat, CGFloat);

struct CGGradientHostObject {
    /// Locations from 0 to 1, in order, and their colors (straight alpha).
    stops: Vec<(CGFloat, Rgba)>,
}
impl HostObject for CGGradientHostObject {}

pub type CGGradientRef = CFTypeRef;

/// `locations` may be null, for colors spread out evenly from 0 to 1.
fn CGGradientCreateWithColorComponents(
    env: &mut Environment,
    space: CGColorSpaceRef,
    components: ConstPtr<CGFloat>,
    locations: ConstPtr<CGFloat>,
    count: GuestUSize,
) -> CGGradientRef {
    // Gray otherwise, which the context only supports alongside RGB.
    let rgb = CGColorSpaceGetModel(env, space) == kCGColorSpaceModelRGB;
    let per_color = if rgb { 4 } else { 2 };
    let mut stops: Vec<(CGFloat, Rgba)> = (0..count)
        .map(|i| {
            let c = |j| env.mem.read(components + i * per_color + j);
            let color = if rgb {
                (c(0), c(1), c(2), c(3))
            } else {
                (c(0), c(0), c(0), c(1))
            };
            let location = if !locations.is_null() {
                env.mem.read(locations + i)
            } else if count > 1 {
                i as CGFloat / (count - 1) as CGFloat
            } else {
                0.0
            };
            (location, color)
        })
        .collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));

    let class = env
        .objc
        .get_known_class("_touchHLE_CGGradient", &mut env.mem);
    env.objc.alloc_object(
        class,
        Box::new(CGGradientHostObject { stops }),
        &mut env.mem,
    )
}

fn CGGradientRetain(env: &mut Environment, gradient: CGGradientRef) -> CGGradientRef {
    if !gradient.is_null() {
        CFRetain(env, gradient)
    } else {
        gradient
    }
}
fn CGGradientRelease(env: &mut Environment, gradient: CGGradientRef) {
    if !gradient.is_null() {
        CFRelease(env, gradient);
    }
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGGradientCreateWithColorComponents(_, _, _, _)),
    export_c_func!(CGGradientRetain(_)),
    export_c_func!(CGGradientRelease(_)),
];

pub(super) fn stops(objc: &ObjC, gradient: CGGradientRef) -> &[(CGFloat, Rgba)] {
    &objc.borrow::<CGGradientHostObject>(gradient).stops
}

/// The color at `t` (0 to 1), between the stops either side of it. Before the
/// first stop and after the last, their colors continue.
pub(super) fn color_at(stops: &[(CGFloat, Rgba)], t: CGFloat) -> Rgba {
    let after = stops.partition_point(|&(location, _)| location <= t);
    if after == 0 {
        return stops[0].1;
    }
    if after == stops.len() {
        return stops[after - 1].1;
    }
    let (l0, c0) = stops[after - 1];
    let (l1, c1) = stops[after];
    let f = if l1 > l0 { (t - l0) / (l1 - l0) } else { 0.0 };
    let mix = |a: CGFloat, b: CGFloat| a + (b - a) * f;
    (
        mix(c0.0, c1.0),
        mix(c0.1, c1.1),
        mix(c0.2, c1.2),
        mix(c0.3, c1.3),
    )
}
