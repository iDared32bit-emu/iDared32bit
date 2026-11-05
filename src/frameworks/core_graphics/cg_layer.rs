/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CGLayer.h`: offscreen drawing, kept in a bitmap context.

use super::cg_bitmap_context::{get_data, CGBitmapContextCreate, CGBitmapContextCreateImage};
use super::cg_color_space::{CGColorSpaceCreateDeviceRGB, CGColorSpaceRelease};
use super::cg_context::{CGContextDrawImage, CGContextRef};
use super::cg_image::{kCGImageAlphaPremultipliedLast, CGImageRelease};
use super::{CGPoint, CGRect, CGSize};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::{CFRelease, CFRetain, CFTypeRef};
use crate::mem::ConstVoidPtr;
use crate::objc::{objc_classes, release, ClassExports, HostObject};
use crate::Environment;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CGLayer is a CFType-based type, but in our implementation those are
// just Objective-C types, so we need a class for it, but its name is not
// visible anywhere.
@implementation _touchHLE_CGLayer: NSObject

- (())dealloc {
    let context = env.objc.borrow::<CGLayerHostObject>(this).context;
    release(env, context);
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

struct CGLayerHostObject {
    size: CGSize,
    /// What's drawn in the layer, at its size.
    context: CGContextRef,
}
impl HostObject for CGLayerHostObject {}

pub type CGLayerRef = CFTypeRef;

/// The layer starts out transparent. It's at the size given, in pixels, as
/// contexts here have no scale.
fn CGLayerCreateWithContext(
    env: &mut Environment,
    _context: CGContextRef,
    size: CGSize,
    _aux_info: ConstVoidPtr, // CFDictionaryRef, reserved
) -> CGLayerRef {
    let width = size.width.max(0.0).ceil() as u32;
    let height = size.height.max(0.0).ceil() as u32;
    let color_space = CGColorSpaceCreateDeviceRGB(env);
    let layer_context = CGBitmapContextCreate(
        env,
        crate::mem::Ptr::null(),
        width,
        height,
        8,
        0,
        color_space,
        kCGImageAlphaPremultipliedLast,
    );
    CGColorSpaceRelease(env, color_space);
    let (_, _, data) = get_data(&env.objc, layer_context);
    env.mem
        .bytes_at_mut(data.cast(), width * height * 4)
        .fill(0);

    let class = env.objc.get_known_class("_touchHLE_CGLayer", &mut env.mem);
    let host_object = CGLayerHostObject {
        size,
        context: layer_context,
    };
    env.objc
        .alloc_object(class, Box::new(host_object), &mut env.mem)
}

fn CGLayerRetain(env: &mut Environment, layer: CGLayerRef) -> CGLayerRef {
    if !layer.is_null() {
        CFRetain(env, layer);
    }
    layer
}

fn CGLayerRelease(env: &mut Environment, layer: CGLayerRef) {
    if !layer.is_null() {
        CFRelease(env, layer);
    }
}

fn CGLayerGetContext(env: &mut Environment, layer: CGLayerRef) -> CGContextRef {
    env.objc.borrow::<CGLayerHostObject>(layer).context
}

fn CGLayerGetSize(env: &mut Environment, layer: CGLayerRef) -> CGSize {
    env.objc.borrow::<CGLayerHostObject>(layer).size
}

/// Draws what's in the layer, scaled to fill `rect`.
fn CGContextDrawLayerInRect(
    env: &mut Environment,
    context: CGContextRef,
    rect: CGRect,
    layer: CGLayerRef,
) {
    let layer_context = CGLayerGetContext(env, layer);
    let image = CGBitmapContextCreateImage(env, layer_context);
    CGContextDrawImage(env, context, rect, image);
    CGImageRelease(env, image);
}

fn CGContextDrawLayerAtPoint(
    env: &mut Environment,
    context: CGContextRef,
    point: CGPoint,
    layer: CGLayerRef,
) {
    let size = CGLayerGetSize(env, layer);
    let rect = CGRect {
        origin: point,
        size,
    };
    CGContextDrawLayerInRect(env, context, rect, layer);
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGLayerCreateWithContext(_, _, _)),
    export_c_func!(CGLayerRetain(_)),
    export_c_func!(CGLayerRelease(_)),
    export_c_func!(CGLayerGetContext(_)),
    export_c_func!(CGLayerGetSize(_)),
    export_c_func!(CGContextDrawLayerInRect(_, _, _)),
    export_c_func!(CGContextDrawLayerAtPoint(_, _, _)),
];
