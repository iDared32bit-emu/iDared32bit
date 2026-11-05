/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CGContext.h`

use super::cg_affine_transform::{CGAffineTransform, CGAffineTransformIdentity};
use super::cg_bitmap_context::{
    CGBitmapContextDrawer, CGBitmapContextGetHeight, CGBitmapContextGetWidth,
};
use super::cg_color::CGColorRef;
use super::cg_color_space::{
    kCGColorSpaceModelMonochrome, kCGColorSpaceModelRGB, CGColorSpaceGetModel, CGColorSpaceModel,
    CGColorSpaceRef,
};
use super::cg_font::{CGFontHostObject, CGFontRef, CGFontRelease, CGFontRetain, CGGlyph};
use super::cg_gradient::CGGradientRef;
use super::cg_image::CGImageRef;
use super::cg_path::{kCGLineCapButt, kCGLineCapRound, kCGLineCapSquare, CGLineCap};
use super::{cg_bitmap_context, cg_color, CGFloat, CGPoint, CGRect, CGSize};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::{CFRelease, CFRetain, CFTypeRef};
use crate::frameworks::uikit;
use crate::mem::{ConstPtr, ConstVoidPtr, GuestUSize};
use crate::objc::{objc_classes, ClassExports, HostObject};
use crate::Environment;
use std::rc::Rc;
use tiny_skia::PathBuilder;

type CGInterpolationQuality = i32;

type CGTextDrawingMode = i32;
const kCGTextFill: CGTextDrawingMode = 0;
const kCGTextFillStroke: CGTextDrawingMode = 2;

pub type CGBlendMode = i32;
pub const kCGBlendModeNormal: CGBlendMode = 0;
pub const kCGBlendModeMultiply: CGBlendMode = 1;
pub const kCGBlendModeScreen: CGBlendMode = 2;
pub const kCGBlendModeOverlay: CGBlendMode = 3;
pub const kCGBlendModeDarken: CGBlendMode = 4;
pub const kCGBlendModeLighten: CGBlendMode = 5;
pub const kCGBlendModeColorDodge: CGBlendMode = 6;
pub const kCGBlendModeColorBurn: CGBlendMode = 7;
pub const kCGBlendModeSoftLight: CGBlendMode = 8;
pub const kCGBlendModeHardLight: CGBlendMode = 9;
pub const kCGBlendModeDifference: CGBlendMode = 10;
pub const kCGBlendModeExclusion: CGBlendMode = 11;
pub const kCGBlendModeHue: CGBlendMode = 12;
pub const kCGBlendModeSaturation: CGBlendMode = 13;
pub const kCGBlendModeColor: CGBlendMode = 14;
pub const kCGBlendModeLuminosity: CGBlendMode = 15;
pub const kCGBlendModeClear: CGBlendMode = 16;
pub const kCGBlendModeCopy: CGBlendMode = 17;
pub const kCGBlendModeSourceIn: CGBlendMode = 18;
pub const kCGBlendModeSourceOut: CGBlendMode = 19;
pub const kCGBlendModeSourceAtop: CGBlendMode = 20;
pub const kCGBlendModeDestinationOver: CGBlendMode = 21;
pub const kCGBlendModeDestinationIn: CGBlendMode = 22;
pub const kCGBlendModeDestinationOut: CGBlendMode = 23;
pub const kCGBlendModeDestinationAtop: CGBlendMode = 24;
pub const kCGBlendModeXOR: CGBlendMode = 25;
pub const kCGBlendModePlusDarker: CGBlendMode = 26;
pub const kCGBlendModePlusLighter: CGBlendMode = 27;

// Helper function for logging blend modes
// It also allowes us to define blend mode conststs we are not using yet!
pub(super) fn blend_mode_name(mode: CGBlendMode) -> &'static str {
    match mode {
        kCGBlendModeNormal => "Normal",
        kCGBlendModeMultiply => "Multiply",
        kCGBlendModeScreen => "Screen",
        kCGBlendModeOverlay => "Overlay",
        kCGBlendModeDarken => "Darken",
        kCGBlendModeLighten => "Lighten",
        kCGBlendModeColorDodge => "ColorDodge",
        kCGBlendModeColorBurn => "ColorBurn",
        kCGBlendModeSoftLight => "SoftLight",
        kCGBlendModeHardLight => "HardLight",
        kCGBlendModeDifference => "Difference",
        kCGBlendModeExclusion => "Exclusion",
        kCGBlendModeHue => "Hue",
        kCGBlendModeSaturation => "Saturation",
        kCGBlendModeColor => "Color",
        kCGBlendModeLuminosity => "Luminosity",
        kCGBlendModeClear => "Clear",
        kCGBlendModeCopy => "Copy",
        kCGBlendModeSourceIn => "SourceIn",
        kCGBlendModeSourceOut => "SourceOut",
        kCGBlendModeSourceAtop => "SourceAtop",
        kCGBlendModeDestinationOver => "DestinationOver",
        kCGBlendModeDestinationIn => "DestinationIn",
        kCGBlendModeDestinationOut => "DestinationOut",
        kCGBlendModeDestinationAtop => "DestinationAtop",
        kCGBlendModeXOR => "XOR",
        kCGBlendModePlusDarker => "PlusDarker",
        kCGBlendModePlusLighter => "PlusLighter",
        _ => "Unknown",
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CGContext seems to be a CFType-based type, but in our implementation those
// are just Objective-C types, so we need a class for it, but its name is not
// visible anywhere.
@implementation _touchHLE_CGContext: NSObject

- (())dealloc {
    let host_obj = env.objc.borrow::<CGContextHostObject>(this);
    let CGContextSubclass::CGBitmapContext(bitmap_data) = host_obj.subclass;
    if bitmap_data.data_is_owned {
        env.mem.free(bitmap_data.data);
    }
    CGFontRelease(env, host_obj.font);

    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

// TODO: keep more states saved once they are implemented
type ContextState = (
    (CGFloat, CGFloat, CGFloat, CGFloat), // RGB fill color
    CGAffineTransform,                    // transform
    CGFontRef,                            // font
    CGFloat,                              // font size
    CGBlendMode,                          // blend mode
    CGLineCap,                            // line cap
    (CGFloat, CGFloat, CGFloat, CGFloat), // RGB stroke color
    CGColorSpaceModel,                    // fill color space
    CGColorSpaceModel,                    // stroke color space
    Option<Rc<tiny_skia::Mask>>,          // clip
);

pub(super) struct CGContextHostObject {
    pub(super) subclass: CGContextSubclass,
    pub(super) rgb_fill_color: (CGFloat, CGFloat, CGFloat, CGFloat),
    pub(super) font: CGFontRef,
    pub(super) font_size: CGFloat,
    /// Current transform.
    pub(super) transform: CGAffineTransform,
    pub(super) blend_mode: CGBlendMode,
    /// Text transform.
    pub(super) text_transform: Option<CGAffineTransform>,
    /// Line/Path data
    pub(super) line_cap: CGLineCap,
    pub(super) line_width: CGFloat,
    pub(super) state_stack: Vec<ContextState>,
    pub(super) rgb_stroke_color: (CGFloat, CGFloat, CGFloat, CGFloat),
    /// The models of the color spaces that CGContextSetFillColor and
    /// CGContextSetStrokeColor take components in.
    pub(super) fill_color_space: CGColorSpaceModel,
    pub(super) stroke_color_space: CGColorSpaceModel,
    /// What drawing is limited to, in device space, if anything.
    pub(super) clip: Option<Rc<tiny_skia::Mask>>,
    /// The current path, in device space: the transform is applied as points
    /// are added, as on iOS. It isn't part of the graphics state.
    pub(super) path: PathBuilder,
}
impl HostObject for CGContextHostObject {}

pub(super) enum CGContextSubclass {
    CGBitmapContext(cg_bitmap_context::CGBitmapContextData),
}

pub type CGContextRef = CFTypeRef;

pub fn CGContextFlush(_env: &mut Environment, ctx: CGContextRef) {
    log!("TODO: CGContextFlush(ctx={:?})", ctx);
}

pub fn CGContextRelease(env: &mut Environment, c: CGContextRef) {
    if !c.is_null() {
        CFRelease(env, c);
    }
}
pub fn CGContextRetain(env: &mut Environment, c: CGContextRef) -> CGContextRef {
    if !c.is_null() {
        CFRetain(env, c)
    } else {
        c
    }
}

pub fn CGContextSelectFont(
    _env: &mut Environment,
    ctx: CGContextRef,
    name_ptr: ConstVoidPtr, // const char * (guest)
    size: CGFloat,
    encoding: i32, // CGTextEncoding
) {
    log!(
        "TODO: CGContextSelectFont(ctx={:?}, name_ptr={:?}, size={}, encoding={})",
        ctx,
        name_ptr,
        size,
        encoding
    );
}

fn CGContextSetBlendMode(env: &mut Environment, context: CGContextRef, blend_mode: CGBlendMode) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .blend_mode = blend_mode;
}

fn CGContextSetFillColorSpace(
    env: &mut Environment,
    context: CGContextRef,
    space: CGColorSpaceRef,
) {
    let color_model = CGColorSpaceGetModel(env, space);
    assert!(color_model == kCGColorSpaceModelMonochrome || color_model == kCGColorSpaceModelRGB);
    // Setting a color space also sets the color to opaque black.
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.fill_color_space = color_model;
    host_obj.rgb_fill_color = (0.0, 0.0, 0.0, 1.0);
}
fn CGContextSetStrokeColorSpace(
    env: &mut Environment,
    context: CGContextRef,
    space: CGColorSpaceRef,
) {
    let color_model = CGColorSpaceGetModel(env, space);
    assert!(color_model == kCGColorSpaceModelMonochrome || color_model == kCGColorSpaceModelRGB);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.stroke_color_space = color_model;
    host_obj.rgb_stroke_color = (0.0, 0.0, 0.0, 1.0);
}

/// Reads a color's components, plus alpha, in a color space's model.
fn read_color(
    env: &Environment,
    model: CGColorSpaceModel,
    components: ConstPtr<CGFloat>,
) -> (CGFloat, CGFloat, CGFloat, CGFloat) {
    let c = |i| env.mem.read(components + i);
    if model == kCGColorSpaceModelRGB {
        (c(0), c(1), c(2), c(3))
    } else {
        (c(0), c(0), c(0), c(1))
    }
}
fn CGContextSetFillColor(
    env: &mut Environment,
    context: CGContextRef,
    components: ConstPtr<CGFloat>,
) {
    let model = env
        .objc
        .borrow::<CGContextHostObject>(context)
        .fill_color_space;
    let color = read_color(env, model, components);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_fill_color = color;
}
fn CGContextSetStrokeColor(
    env: &mut Environment,
    context: CGContextRef,
    components: ConstPtr<CGFloat>,
) {
    let model = env
        .objc
        .borrow::<CGContextHostObject>(context)
        .stroke_color_space;
    let color = read_color(env, model, components);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_stroke_color = color;
}

pub fn CGContextSetCharacterSpacing(_env: &mut Environment, ctx: CGContextRef, spacing: CGFloat) {
    log!(
        "TODO: CGContextSetCharacterSpacing(ctx={:?}, spacing={})",
        ctx,
        spacing
    );
}

fn CGContextSetFillColorWithColor(env: &mut Environment, context: CGContextRef, color: CGColorRef) {
    let (r, g, b, a) = cg_color::to_rgba(&env.objc, color);
    CGContextSetRGBFillColor(env, context, r, g, b, a)
}

pub fn CGContextSetRGBFillColor(
    env: &mut Environment,
    context: CGContextRef,
    red: CGFloat,
    green: CGFloat,
    blue: CGFloat,
    alpha: CGFloat,
) {
    let color = (red, green, blue, alpha);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_fill_color = color;
}

fn CGContextSetGrayFillColor(
    env: &mut Environment,
    context: CGContextRef,
    gray: CGFloat,
    alpha: CGFloat,
) {
    let color = (gray, gray, gray, alpha);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_fill_color = color;
}

fn CGContextSetGrayStrokeColor(
    _env: &mut Environment,
    context: CGContextRef,
    gray: CGFloat,
    alpha: CGFloat,
) {
    log!(
        "TODO: CGContextSetGrayStrokeColor({:?}, {}, {})",
        context,
        gray,
        alpha,
    );
}
fn CGContextSetRGBStrokeColor(
    env: &mut Environment,
    context: CGContextRef,
    r: CGFloat,
    g: CGFloat,
    b: CGFloat,
    a: CGFloat,
) {
    let color = (r, g, b, a);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_stroke_color = color;
}

fn CGContextSetShadowWithColor(
    _env: &mut Environment,
    context: CGContextRef,
    offset: CGSize,
    blur: CGFloat,
    color: CGColorRef,
) {
    log!(
        "TODO: CGContextSetShadowWithColor({:?}, {}, {}, {:?})",
        context,
        offset,
        blur,
        color
    );
}

pub fn CGContextFillRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    cg_bitmap_context::fill_rect(env, context, rect, /* clear: */ false);
}

pub fn CGContextClearRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    cg_bitmap_context::fill_rect(env, context, rect, /* clear: */ true);
}

pub fn CGContextFillEllipseInRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    cg_bitmap_context::fill_ellipse(env, context, rect);
}

fn CGContextClipToRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    clip_to_rects(env, context, &[rect]);
}

fn CGContextClipToRects(
    env: &mut Environment,
    context: CGContextRef,
    rects: ConstPtr<CGRect>,
    count: GuestUSize,
) {
    let rects: Vec<CGRect> = (0..count).map(|i| env.mem.read(rects + i)).collect();
    clip_to_rects(env, context, &rects);
}

/// Limits drawing to where it's already allowed and inside one of the rects.
fn clip_to_rects(env: &mut Environment, context: CGContextRef, rects: &[CGRect]) {
    log_dbg!("Clipping {:?} to {:?}", context, rects);
    let width = CGBitmapContextGetWidth(env, context);
    let height = CGBitmapContextGetHeight(env, context);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    let mut builder = PathBuilder::new();
    for rect in rects {
        if let Some(rect) = tiny_skia::Rect::from_xywh(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        ) {
            builder.push_rect(rect);
        }
    }
    let transform = cg_bitmap_context::to_skia_transform(host_obj.transform);
    let mut mask = match host_obj.clip.take() {
        Some(clip) => Rc::unwrap_or_clone(clip),
        None => {
            // An empty bitmap, which nothing can be drawn in anyway.
            let Some(mut mask) = tiny_skia::Mask::new(width, height) else {
                return;
            };
            mask.data_mut().fill(255);
            mask
        }
    };
    match builder.finish() {
        Some(path) => {
            mask.intersect_path(&path, tiny_skia::FillRule::Winding, true, transform);
        }
        // Nothing (e.g. only empty rects): nothing can be drawn.
        None => mask.data_mut().fill(0),
    }
    host_obj.clip = Some(Rc::new(mask));
}

pub fn CGContextConcatCTM(
    env: &mut Environment,
    context: CGContextRef,
    transform: CGAffineTransform,
) {
    log_dbg!("CGContextConcatCTM({:?})", transform);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = transform.concat(host_obj.transform);
}
pub fn CGContextGetCTM(env: &mut Environment, context: CGContextRef) -> CGAffineTransform {
    let res = env.objc.borrow::<CGContextHostObject>(context).transform;
    log_dbg!("CGContextGetCTM() => {:?}", res);
    res
}
/// Not a real Core Graphics function: replaces the CTM, for host code that
/// changes it temporarily.
pub fn set_ctm(env: &mut Environment, context: CGContextRef, transform: CGAffineTransform) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .transform = transform;
}
pub fn CGContextGetUserSpaceToDeviceSpaceTransform(
    env: &mut Environment,
    context: CGContextRef,
) -> CGAffineTransform {
    let ctm = CGContextGetCTM(env, context);
    let host_obj = env.objc.borrow::<CGContextHostObject>(context);
    #[allow(unreachable_patterns)]
    match &host_obj.subclass {
        CGContextSubclass::CGBitmapContext(_) => {
            let height = CGBitmapContextGetHeight(env, context) as CGFloat;
            let flip = CGAffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: -1.0,
                tx: 0.0,
                ty: height,
            };
            flip.concat(ctm)
        }
        _ => ctm,
    }
}
pub fn CGContextRotateCTM(env: &mut Environment, context: CGContextRef, angle: CGFloat) {
    log_dbg!("CGContextRotateCTM({:?})", angle);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = host_obj.transform.rotate(angle);
}
pub fn CGContextScaleCTM(env: &mut Environment, context: CGContextRef, x: CGFloat, y: CGFloat) {
    log_dbg!("CGContextScaleCTM({:?})", (x, y));
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = host_obj.transform.scale(x, y);
}
pub fn CGContextTranslateCTM(
    env: &mut Environment,
    context: CGContextRef,
    tx: CGFloat,
    ty: CGFloat,
) {
    log_dbg!("CGContextTranslateCTM({:?})", (tx, ty));
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = host_obj.transform.translate(tx, ty);
}

pub fn CGContextDrawImage(
    env: &mut Environment,
    context: CGContextRef,
    rect: CGRect,
    image: CGImageRef,
) {
    cg_bitmap_context::draw_image(env, context, rect, image);
}

fn CGContextSaveGState(env: &mut Environment, context: CGContextRef) {
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.state_stack.push((
        host_obj.rgb_fill_color,
        host_obj.transform,
        host_obj.font,
        host_obj.font_size,
        host_obj.blend_mode,
        host_obj.line_cap,
        host_obj.rgb_stroke_color,
        host_obj.fill_color_space,
        host_obj.stroke_color_space,
        host_obj.clip.clone(),
    ));
    CGFontRetain(env, env.objc.borrow::<CGContextHostObject>(context).font);
}

fn CGContextRestoreGState(env: &mut Environment, context: CGContextRef) {
    // We need to release _old_ font, there are 2 cases:
    // - font hasn't been set between save/restore -> this release corresponds
    // the font retain from save
    // - font has been set between save/restore -> we need to release old font
    // retained on the set
    CGFontRelease(env, env.objc.borrow::<CGContextHostObject>(context).font);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    let state = host_obj.state_stack.pop().unwrap();
    host_obj.rgb_fill_color = state.0;
    host_obj.transform = state.1;
    host_obj.font = state.2;
    host_obj.font_size = state.3;
    host_obj.blend_mode = state.4;
    host_obj.line_cap = state.5;
    host_obj.rgb_stroke_color = state.6;
    host_obj.fill_color_space = state.7;
    host_obj.stroke_color_space = state.8;
    host_obj.clip = state.9;
}

fn CGContextSetInterpolationQuality(
    _env: &mut Environment,
    context: CGContextRef,
    quality: CGInterpolationQuality,
) {
    log!(
        "TODO: CGContextSetInterpolationQuality({:?}, {:?})",
        context,
        quality
    );
}
fn CGContextSetAllowsAntialiasing(_env: &mut Environment, context: CGContextRef, allow: bool) {
    log!(
        "TODO: CGContextSetAllowsAntialiasing({:?}, {})",
        context,
        allow
    );
}

fn CGContextSetShouldSmoothFonts(_env: &mut Environment, context: CGContextRef, should: bool) {
    log!(
        "TODO: CGContextSetShouldSmoothFonts({:?}, {})",
        context,
        should
    );
}

fn CGContextSetAlpha(_env: &mut Environment, context: CGContextRef, alpha: CGFloat) {
    // Since 1.0 is the default value for alpha, we can just ignore it
    if alpha != 1.0 {
        log!("TODO: CGContextSetAlpha({:?}, {:?})", context, alpha);
    }
}

fn CGContextSetFont(env: &mut Environment, context: CGContextRef, font: CGFontRef) {
    CGFontRetain(env, font);
    let old_font = env.objc.borrow_mut::<CGContextHostObject>(context).font;
    CGFontRelease(env, old_font);
    env.objc.borrow_mut::<CGContextHostObject>(context).font = font;
}

fn CGContextSetFontSize(env: &mut Environment, context: CGContextRef, size: CGFloat) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .font_size = size;
}

fn CGContextSetTextDrawingMode(
    _env: &mut Environment,
    _context: CGContextRef,
    mode: CGTextDrawingMode,
) {
    assert!(mode == kCGTextFill || mode == kCGTextFillStroke); // TODO: support other modes
}

fn CGContextSetTextMatrix(
    env: &mut Environment,
    context: CGContextRef,
    transform: CGAffineTransform,
) {
    log_dbg!("CGContextSetTextMatrix({:?})", transform);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .text_transform = Some(transform);
}

fn CGContextShowGlyphsAtPoint(
    env: &mut Environment,
    context: CGContextRef,
    x: CGFloat,
    y: CGFloat,
    glyphs: ConstPtr<CGGlyph>,
    count: GuestUSize,
) {
    let mut glyph_ids = Vec::new();
    for i in 0..count {
        let glyph_id = env.mem.read(glyphs + i);
        glyph_ids.push(rusttype::GlyphId(glyph_id));
    }

    let font = env.objc.borrow::<CGContextHostObject>(context).font;
    let font_size = env.objc.borrow::<CGContextHostObject>(context).font_size;
    let text_transform = env
        .objc
        .borrow::<CGContextHostObject>(context)
        .text_transform
        .unwrap_or(CGAffineTransformIdentity);

    let font = &env.objc.borrow::<CGFontHostObject>(font).font;

    let mut drawer = CGBitmapContextDrawer::new(&env.objc, &mut env.mem, context);
    let fill_color = drawer.rgb_fill_color();

    font.draw_glyphs(
        font_size,
        glyph_ids,
        (x, y),
        text_transform,
        |raster_glyph| {
            uikit::ui_font::draw_font_glyph(
                &mut drawer,
                raster_glyph,
                fill_color,
                /* clip_x: */ None,
                /* clip_y: */ None,
            )
        },
    );
}

fn CGContextShowGlyphsAtPositions(
    env: &mut Environment,
    context: CGContextRef,
    glyphs: ConstPtr<CGGlyph>,
    positions: ConstPtr<CGPoint>,
    count: GuestUSize,
) {
    let text_transform = env
        .objc
        .borrow::<CGContextHostObject>(context)
        .text_transform
        .unwrap_or(CGAffineTransformIdentity);
    assert!(text_transform.tx == 0.0 && text_transform.ty == 0.0); // TODO

    for i in 0..count {
        let glyph_ptr = glyphs + i;
        let point = env.mem.read(positions + i);
        let transformed_point = text_transform.apply_to_point(point);
        CGContextShowGlyphsAtPoint(
            env,
            context,
            transformed_point.x,
            transformed_point.y,
            glyph_ptr,
            1,
        );
    }
}

fn CGContextSetLineCap(env: &mut Environment, context: CGContextRef, cap: CGLineCap) {
    log_dbg!("CGContextSetLineCap({:?}, {})", context, cap);

    assert!(cap == kCGLineCapButt || cap == kCGLineCapRound || cap == kCGLineCapSquare);
    env.objc.borrow_mut::<CGContextHostObject>(context).line_cap = cap;
}

fn CGContextSetLineWidth(env: &mut Environment, context: CGContextRef, width: CGFloat) {
    log_dbg!("CGContextSetLineCap({:?}, {})", context, width,);

    assert!(width > 0.0);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .line_width = width;
}

fn CGContextSetStrokeColorWithColor(
    env: &mut Environment,
    context: CGContextRef,
    color: CGColorRef,
) {
    let (r, g, b, a) = cg_color::to_rgba(&env.objc, color);
    CGContextSetRGBStrokeColor(env, context, r, g, b, a)
}

/// Applies the current transform, which the path is stored with.
fn device_point(env: &Environment, context: CGContextRef, x: CGFloat, y: CGFloat) -> CGPoint {
    let transform = env.objc.borrow::<CGContextHostObject>(context).transform;
    transform.apply_to_point(CGPoint { x, y })
}
fn path(env: &mut Environment, context: CGContextRef) -> &mut PathBuilder {
    &mut env.objc.borrow_mut::<CGContextHostObject>(context).path
}

fn CGContextBeginPath(env: &mut Environment, context: CGContextRef) {
    *path(env, context) = PathBuilder::new();
}

fn CGContextMoveToPoint(env: &mut Environment, context: CGContextRef, x: CGFloat, y: CGFloat) {
    let p = device_point(env, context, x, y);
    path(env, context).move_to(p.x, p.y);
}

fn CGContextAddLineToPoint(env: &mut Environment, context: CGContextRef, x: CGFloat, y: CGFloat) {
    let p = device_point(env, context, x, y);
    // Without a current point, iOS ignores it.
    if !path(env, context).is_empty() {
        path(env, context).line_to(p.x, p.y);
    }
}

#[allow(clippy::too_many_arguments)]
fn CGContextAddCurveToPoint(
    env: &mut Environment,
    context: CGContextRef,
    cp1x: CGFloat,
    cp1y: CGFloat,
    cp2x: CGFloat,
    cp2y: CGFloat,
    x: CGFloat,
    y: CGFloat,
) {
    let cp1 = device_point(env, context, cp1x, cp1y);
    let cp2 = device_point(env, context, cp2x, cp2y);
    let p = device_point(env, context, x, y);
    if !path(env, context).is_empty() {
        path(env, context).cubic_to(cp1.x, cp1.y, cp2.x, cp2.y, p.x, p.y);
    }
}

fn CGContextClosePath(env: &mut Environment, context: CGContextRef) {
    path(env, context).close();
}

fn CGContextAddRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    let CGRect { origin, size } = rect;
    let corners = [
        (origin.x, origin.y),
        (origin.x + size.width, origin.y),
        (origin.x + size.width, origin.y + size.height),
        (origin.x, origin.y + size.height),
    ];
    for (i, (x, y)) in corners.into_iter().enumerate() {
        let p = device_point(env, context, x, y);
        if i == 0 {
            path(env, context).move_to(p.x, p.y);
        } else {
            path(env, context).line_to(p.x, p.y);
        }
    }
    path(env, context).close();
}

fn CGContextAddEllipseInRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    let Some(oval) = tiny_skia::Rect::from_xywh(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
    .and_then(PathBuilder::from_oval) else {
        return;
    };
    let transform = env.objc.borrow::<CGContextHostObject>(context).transform;
    let Some(oval) = oval.transform(cg_bitmap_context::to_skia_transform(transform)) else {
        return;
    };
    let builder = path(env, context);
    for segment in oval.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(p) => builder.move_to(p.x, p.y),
            tiny_skia::PathSegment::LineTo(p) => builder.line_to(p.x, p.y),
            tiny_skia::PathSegment::QuadTo(p1, p) => builder.quad_to(p1.x, p1.y, p.x, p.y),
            tiny_skia::PathSegment::CubicTo(p1, p2, p) => {
                builder.cubic_to(p1.x, p1.y, p2.x, p2.y, p.x, p.y)
            }
            tiny_skia::PathSegment::Close => builder.close(),
        }
    }
}

/// Adds a line to where an arc of `radius` meets the line from the current
/// point to (x1, y1), then the arc, ending where it meets the line from
/// (x1, y1) to (x2, y2).
#[allow(clippy::too_many_arguments)]
fn CGContextAddArcToPoint(
    env: &mut Environment,
    context: CGContextRef,
    x1: CGFloat,
    y1: CGFloat,
    x2: CGFloat,
    y2: CGFloat,
    radius: CGFloat,
) {
    let transform = env.objc.borrow::<CGContextHostObject>(context).transform;
    let Some(current) = path(env, context).last_point() else {
        return;
    };
    // The arc is worked out in user space.
    let p0 = transform.invert().apply_to_point(CGPoint {
        x: current.x,
        y: current.y,
    });
    let (v1x, v1y) = (p0.x - x1, p0.y - y1);
    let (v2x, v2y) = (x2 - x1, y2 - y1);
    let (l1, l2) = (v1x.hypot(v1y), v2x.hypot(v2y));
    let cross = v1x * v2y - v1y * v2x;
    if radius <= 0.0 || l1 == 0.0 || l2 == 0.0 || cross.abs() < 1e-6 * l1 * l2 {
        CGContextAddLineToPoint(env, context, x1, y1);
        return;
    }
    let (u1x, u1y, u2x, u2y) = (v1x / l1, v1y / l1, v2x / l2, v2y / l2);
    // The angle between the lines, and the distance from their corner to
    // where the arc meets them.
    let angle = (u1x * u2x + u1y * u2y).clamp(-1.0, 1.0).acos();
    let distance = radius / (angle / 2.0).tan();
    let start = (x1 + u1x * distance, y1 + u1y * distance);
    let end = (x1 + u2x * distance, y1 + u2y * distance);
    let (bx, by) = (u1x + u2x, u1y + u2y);
    let bisector = bx.hypot(by);
    let center_distance = radius / (angle / 2.0).sin();
    let center = (
        x1 + bx / bisector * center_distance,
        y1 + by / bisector * center_distance,
    );

    CGContextAddLineToPoint(env, context, start.0, start.1);
    let start_angle = (start.1 - center.1).atan2(start.0 - center.0);
    let end_angle = (end.1 - center.1).atan2(end.0 - center.0);
    let mut sweep = end_angle - start_angle;
    if sweep > std::f32::consts::PI {
        sweep -= std::f32::consts::TAU;
    } else if sweep < -std::f32::consts::PI {
        sweep += std::f32::consts::TAU;
    }
    // Cubic Béziers of at most 90° each.
    let count = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0);
    let step = sweep / count;
    let k = 4.0 / 3.0 * (step / 4.0).tan() * radius;
    for i in 0..count as u32 {
        let a = start_angle + step * i as f32;
        let b = a + step;
        let (p_a, p_b) = (
            (center.0 + radius * a.cos(), center.1 + radius * a.sin()),
            (center.0 + radius * b.cos(), center.1 + radius * b.sin()),
        );
        CGContextAddCurveToPoint(
            env,
            context,
            p_a.0 - k * a.sin(),
            p_a.1 + k * a.cos(),
            p_b.0 + k * b.sin(),
            p_b.1 - k * b.cos(),
            p_b.0,
            p_b.1,
        );
    }
}

fn CGContextFillPath(env: &mut Environment, context: CGContextRef) {
    let path = std::mem::take(path(env, context));
    if let Some(path) = path.finish() {
        cg_bitmap_context::draw_path(env, context, &path, /* stroke: */ false);
    }
}

fn CGContextStrokePath(env: &mut Environment, context: CGContextRef) {
    let path = std::mem::take(path(env, context));
    if let Some(path) = path.finish() {
        cg_bitmap_context::draw_path(env, context, &path, /* stroke: */ true);
    }
}

fn CGContextStrokeRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    // Like on iOS, this replaces the current path.
    CGContextBeginPath(env, context);
    CGContextAddRect(env, context, rect);
    CGContextStrokePath(env, context);
}

/// Strokes a line between each pair of points.
fn CGContextStrokeLineSegments(
    env: &mut Environment,
    context: CGContextRef,
    points: ConstPtr<CGPoint>,
    count: GuestUSize,
) {
    CGContextBeginPath(env, context);
    for i in 0..count / 2 {
        let a: CGPoint = env.mem.read(points + 2 * i);
        let b: CGPoint = env.mem.read(points + 2 * i + 1);
        CGContextMoveToPoint(env, context, a.x, a.y);
        CGContextAddLineToPoint(env, context, b.x, b.y);
    }
    CGContextStrokePath(env, context);
}

type CGGradientDrawingOptions = u32;

fn CGContextDrawLinearGradient(
    env: &mut Environment,
    context: CGContextRef,
    gradient: CGGradientRef,
    start: CGPoint,
    end: CGPoint,
    options: CGGradientDrawingOptions,
) {
    cg_bitmap_context::draw_linear_gradient(env, context, gradient, start, end, options);
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGContextFlush(_)),
    export_c_func!(CGContextRetain(_)),
    export_c_func!(CGContextSelectFont(_, _, _, _)),
    export_c_func!(CGContextRelease(_)),
    export_c_func!(CGContextSetBlendMode(_, _)),
    export_c_func!(CGContextSetFillColorSpace(_, _)),
    export_c_func!(CGContextSetStrokeColorSpace(_, _)),
    export_c_func!(CGContextSetFillColor(_, _)),
    export_c_func!(CGContextSetStrokeColor(_, _)),
    export_c_func!(CGContextSetCharacterSpacing(_, _)),
    export_c_func!(CGContextSetFillColorWithColor(_, _)),
    export_c_func!(CGContextSetRGBFillColor(_, _, _, _, _)),
    export_c_func!(CGContextSetGrayFillColor(_, _, _)),
    export_c_func!(CGContextSetGrayStrokeColor(_, _, _)),
    export_c_func!(CGContextSetStrokeColorWithColor(_, _)),
    export_c_func!(CGContextSetRGBStrokeColor(_, _, _, _, _)),
    export_c_func!(CGContextSetShadowWithColor(_, _, _, _)),
    export_c_func!(CGContextFillRect(_, _)),
    export_c_func!(CGContextClearRect(_, _)),
    export_c_func!(CGContextClipToRect(_, _)),
    export_c_func!(CGContextClipToRects(_, _, _)),
    export_c_func!(CGContextConcatCTM(_, _)),
    export_c_func!(CGContextGetCTM(_)),
    export_c_func!(CGContextGetUserSpaceToDeviceSpaceTransform(_)),
    export_c_func!(CGContextRotateCTM(_, _)),
    export_c_func!(CGContextScaleCTM(_, _, _)),
    export_c_func!(CGContextTranslateCTM(_, _, _)),
    export_c_func!(CGContextDrawImage(_, _, _)),
    export_c_func!(CGContextSaveGState(_)),
    export_c_func!(CGContextRestoreGState(_)),
    export_c_func!(CGContextSetInterpolationQuality(_, _)),
    export_c_func!(CGContextSetAllowsAntialiasing(_, _)),
    export_c_func!(CGContextSetShouldSmoothFonts(_, _)),
    export_c_func!(CGContextSetAlpha(_, _)),
    export_c_func!(CGContextSetFont(_, _)),
    export_c_func!(CGContextSetFontSize(_, _)),
    export_c_func!(CGContextSetTextDrawingMode(_, _)),
    export_c_func!(CGContextSetTextMatrix(_, _)),
    export_c_func!(CGContextShowGlyphsAtPoint(_, _, _, _, _)),
    export_c_func!(CGContextShowGlyphsAtPositions(_, _, _, _)),
    export_c_func!(CGContextSetLineCap(_, _)),
    export_c_func!(CGContextSetLineWidth(_, _)),
    export_c_func!(CGContextBeginPath(_)),
    export_c_func!(CGContextMoveToPoint(_, _, _)),
    export_c_func!(CGContextAddLineToPoint(_, _, _)),
    export_c_func!(CGContextAddCurveToPoint(_, _, _, _, _, _, _)),
    export_c_func!(CGContextAddArcToPoint(_, _, _, _, _, _)),
    export_c_func!(CGContextAddRect(_, _)),
    export_c_func!(CGContextAddEllipseInRect(_, _)),
    export_c_func!(CGContextClosePath(_)),
    export_c_func!(CGContextFillPath(_)),
    export_c_func!(CGContextStrokePath(_)),
    export_c_func!(CGContextStrokeRect(_, _)),
    export_c_func!(CGContextStrokeLineSegments(_, _, _)),
    export_c_func!(CGContextDrawLinearGradient(_, _, _, _, _)),
    export_c_func!(CGContextFillEllipseInRect(_, _)),
];
