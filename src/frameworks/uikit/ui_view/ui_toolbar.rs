/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIToolbar`.

use crate::frameworks::uikit::ui_bar_item::ui_bar_button_item::{
    UIBarButtonItemHostObject, UIBarButtonItemStyleDone, UIBarButtonItemStylePlain,
    UIBarButtonSystemItemFixedSpace, UIBarButtonSystemItemFlexibleSpace,
};
use crate::frameworks::{
    core_graphics::{cg_image, CGFloat, CGPoint, CGRect, CGSize},
    foundation::{ns_array, NSInteger, NSUInteger},
};
use crate::image::Image;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes,
    release, retain, ClassExports, NSZonePtr,
};
use crate::Environment;

pub type UIBarStyle = NSInteger;

pub const UIBarStyleDefault: UIBarStyle = 0;
pub const UIBarStyleBlack: UIBarStyle = 1;
pub const UIBarStyleBlackTranslucent: UIBarStyle = 2;

pub struct UIToolbarHostObject {
    superclass: super::UIViewHostObject,
    items: Vec<id>,
    /// Internal views (UIControl*) corresponding to each item,
    item_views: Vec<id>,
    bar_style: UIBarStyle,
}
impl_HostObject_with_superclass!(UIToolbarHostObject);

impl Default for UIToolbarHostObject {
    fn default() -> Self {
        Self {
            superclass: Default::default(),
            items: Vec::new(),
            item_views: Vec::new(),
            bar_style: UIBarStyleDefault,
        }
    }
}

/// iPhone OS 3's glossy gradient: the colours (sRGB RGBA) at the top and
/// middle of the upper half, then at the middle and bottom of the lower half.
type Gloss = [[f32; 4]; 4];

const BLACK_BAR: Gloss = [
    [0.33, 0.33, 0.33, 1.0],
    [0.16, 0.16, 0.16, 1.0],
    [0.03, 0.03, 0.03, 1.0],
    [0.0, 0.0, 0.0, 1.0],
];
const BLACK_TRANSLUCENT_BAR: Gloss = [
    [0.25, 0.25, 0.25, 0.55],
    [0.1, 0.1, 0.1, 0.55],
    [0.0, 0.0, 0.0, 0.6],
    [0.0, 0.0, 0.0, 0.6],
];
const DEFAULT_BAR: Gloss = [
    [0.69, 0.74, 0.8, 1.0],
    [0.55, 0.61, 0.7, 1.0],
    [0.51, 0.58, 0.68, 1.0],
    [0.43, 0.52, 0.63, 1.0],
];
const BLACK_BUTTON: Gloss = [
    [0.44, 0.44, 0.44, 1.0],
    [0.23, 0.23, 0.23, 1.0],
    [0.0, 0.0, 0.0, 1.0],
    [0.0, 0.0, 0.0, 1.0],
];
const DEFAULT_BUTTON: Gloss = [
    [0.55, 0.62, 0.73, 1.0],
    [0.43, 0.52, 0.64, 1.0],
    [0.35, 0.45, 0.6, 1.0],
    [0.37, 0.47, 0.62, 1.0],
];
const DONE_BUTTON: Gloss = [
    [0.47, 0.64, 0.95, 1.0],
    [0.3, 0.51, 0.92, 1.0],
    [0.15, 0.38, 0.86, 1.0],
    [0.15, 0.38, 0.86, 1.0],
];

/// Height of a bordered bar button, as on iPhone OS 3.
pub(super) const BUTTON_HEIGHT: CGFloat = 30.0;
/// Space between the toolbar's edges and its first and last items.
const EDGE_INSET: CGFloat = 6.0;

fn gloss_at(gloss: &Gloss, t: f32) -> [f32; 4] {
    let (a, b, t) = if t < 0.5 {
        (gloss[0], gloss[1], t * 2.0)
    } else {
        (gloss[2], gloss[3], t * 2.0 - 1.0)
    };
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Premultiplied RGBA pixels that shapes are drawn onto.
struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        let pixels = vec![[0.0; 4]; (width * height) as usize];
        Canvas {
            width,
            height,
            pixels,
        }
    }

    /// Draw an anti-aliased rounded rectangle (in pixels), filled with `fill`
    /// from the top (0) to the bottom (1) of its inside, and optionally with
    /// a border of the given width, shaded from one colour at the top to
    /// another at the bottom.
    fn rounded_rect(
        &mut self,
        rect: CGRect,
        radius: f32,
        border: Option<(f32, [f32; 4], [f32; 4])>,
        fill: impl Fn(f32) -> [f32; 4],
    ) {
        let CGRect { origin, size } = rect;
        let (cx, cy) = (origin.x + size.width / 2.0, origin.y + size.height / 2.0);
        let radius = radius.min(size.width / 2.0).min(size.height / 2.0);
        let (border_width, border_top, border_bottom) = border.unwrap_or((0.0, [0.0; 4], [0.0; 4]));
        // Signed distance from the edge: negative inside.
        let distance = |px: f32, py: f32| {
            let qx = (px - cx).abs() - (size.width / 2.0 - radius);
            let qy = (py - cy).abs() - (size.height / 2.0 - radius);
            qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
        };
        // Exact coverage of each pixel from a 4x4 grid of samples, so small
        // corners look right even at 1x.
        const SAMPLES: usize = 4;
        let inner_height = size.height - border_width * 2.0;
        for y in 0..self.height {
            let py = y as f32 + 0.5;
            let t = ((py - origin.y - border_width) / inner_height).clamp(0.0, 1.0);
            let fill_color = fill(t);
            let t = ((py - origin.y) / size.height).clamp(0.0, 1.0);
            let border_color: [f32; 4] =
                std::array::from_fn(|i| border_top[i] + (border_bottom[i] - border_top[i]) * t);
            for x in 0..self.width {
                let (mut outer, mut inner) = (0, 0);
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let d = distance(
                            x as f32 + (sx as f32 + 0.5) / SAMPLES as f32,
                            y as f32 + (sy as f32 + 0.5) / SAMPLES as f32,
                        );
                        outer += (d <= 0.0) as usize;
                        inner += (d <= -border_width) as usize;
                    }
                }
                if outer == 0 {
                    continue;
                }
                // Mix the border and the fill by how much of each is covered,
                // then draw the mix with the shape's coverage.
                let fill_share = inner as f32 / outer as f32;
                let c: [f32; 4] = std::array::from_fn(|i| {
                    border_color[i] + (fill_color[i] - border_color[i]) * fill_share
                });
                let coverage = outer as f32 / (SAMPLES * SAMPLES) as f32 * c[3];
                let dst = &mut self.pixels[(y * self.width + x) as usize];
                for i in 0..3 {
                    dst[i] = c[i] * coverage + dst[i] * (1.0 - coverage);
                }
                dst[3] = coverage + dst[3] * (1.0 - coverage);
            }
        }
    }

    fn into_image(self) -> Image {
        let bytes = self
            .pixels
            .iter()
            .flatten()
            .map(|&v| (v * 255.0).round() as u8);
        Image::from_pixel_vec(bytes.collect(), (self.width, self.height))
    }
}

/// Scale to draw at, so edges stay sharp when the screen is scaled up.
fn draw_scale(env: &Environment) -> f32 {
    let ui_scale = env.window.as_ref().map_or(1.0, |window| window.ui_scale());
    env.options.scale_hack.get() as f32 * ui_scale
}

/// Whether `view`'s layer already has an image of this size.
fn has_image_of_size(env: &mut Environment, view: id, width: u32, height: u32) -> bool {
    let layer: id = msg![env; view layer];
    let contents: id = msg![env; layer contents];
    contents != nil && cg_image::borrow_image(&env.objc, contents).dimensions() == (width, height)
}

fn set_layer_image(env: &mut Environment, view: id, canvas: Canvas) {
    let image = cg_image::from_image(env, canvas.into_image());
    let layer: id = msg![env; view layer];
    () = msg![env; layer setContents:image];
    release(env, image);
}

/// A bar's glossy background, with a thin line along its top (toolbars) or
/// bottom (navigation bars), in pixels.
fn background_canvas(
    width: u32,
    height: u32,
    scale: f32,
    bar_style: UIBarStyle,
    line_at_top: bool,
) -> Canvas {
    let (gloss, line) = match bar_style {
        UIBarStyleBlack => (BLACK_BAR, [0.55, 0.55, 0.55, 0.5]),
        UIBarStyleBlackTranslucent => (BLACK_TRANSLUCENT_BAR, [0.55, 0.55, 0.55, 0.5]),
        _ => (DEFAULT_BAR, [0.29, 0.33, 0.4, 1.0]),
    };
    let mut canvas = Canvas::new(width, height);
    let size = CGSize {
        width: width as f32,
        height: height as f32,
    };
    let full = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size,
    };
    canvas.rounded_rect(full, 0.0, None, |t| gloss_at(&gloss, t));
    let line_rect = CGRect {
        origin: CGPoint {
            x: 0.0,
            y: if line_at_top {
                0.0
            } else {
                size.height - scale
            },
        },
        size: CGSize {
            height: scale,
            ..size
        },
    };
    canvas.rounded_rect(line_rect, 0.0, None, |_| line);
    canvas
}

/// A bordered (or Done) bar button's background, in pixels: a glossy rounded
/// rectangle with a thin border, and on the default bar a faint highlight
/// below it.
fn button_canvas(
    width: u32,
    height: u32,
    scale: f32,
    bar_style: UIBarStyle,
    item_style: NSInteger,
) -> Canvas {
    let black = bar_style != UIBarStyleDefault;
    // On black bars the border is dark at the top and light at the bottom,
    // as on iPhone OS 3, so the button looks set into the bar.
    let (gloss, border_top, border_bottom) = if item_style == UIBarButtonItemStyleDone {
        let border = [0.05, 0.13, 0.37, 1.0];
        (DONE_BUTTON, border, border)
    } else if black {
        (BLACK_BUTTON, [0.1, 0.1, 0.09, 1.0], [0.39, 0.39, 0.38, 1.0])
    } else {
        let border = [0.18, 0.25, 0.37, 1.0];
        (DEFAULT_BUTTON, border, border)
    };

    let mut canvas = Canvas::new(width, height);
    let (w, h) = (width as f32, height as f32);
    let radius = 3.5 * scale;
    let body = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: w,
            // The highlight below takes the last point, on the default bar.
            height: if black { h } else { h - scale },
        },
    };
    let below = CGRect {
        origin: CGPoint { x: 0.0, y: scale },
        ..body
    };
    // Only on the default bar: on black bars it shows as a ghost edge
    // around the bottom corners, which real ones don't have.
    if !black {
        canvas.rounded_rect(below, radius, None, |_| [1.0, 1.0, 1.0, 0.3]);
    }
    canvas.rounded_rect(
        body,
        radius,
        Some((scale, border_top, border_bottom)),
        |t| gloss_at(&gloss, t),
    );
    canvas
}

/// `view`'s size in pixels at [draw_scale], unless it already has an image
/// of that size (or is empty).
fn size_to_draw(env: &mut Environment, view: id) -> Option<(u32, u32, f32)> {
    let bounds: CGRect = msg![env; view bounds];
    let scale = draw_scale(env);
    let width = (bounds.size.width * scale).round() as u32;
    let height = (bounds.size.height * scale).round() as u32;
    if width == 0 || height == 0 || has_image_of_size(env, view, width, height) {
        return None;
    }
    Some((width, height, scale))
}

pub(super) fn draw_bar_background(
    env: &mut Environment,
    this: id,
    bar_style: UIBarStyle,
    line_at_top: bool,
) {
    let Some((width, height, scale)) = size_to_draw(env, this) else {
        return;
    };
    let canvas = background_canvas(width, height, scale, bar_style, line_at_top);
    set_layer_image(env, this, canvas);
}

pub(super) fn draw_button(
    env: &mut Environment,
    view: id,
    bar_style: UIBarStyle,
    item_style: NSInteger,
) {
    let Some((width, height, scale)) = size_to_draw(env, view) else {
        return;
    };
    let canvas = button_canvas(width, height, scale, bar_style, item_style);
    set_layer_image(env, view, canvas);
}

const PLAIN_SEGMENT: Gloss = [
    [1.0, 1.0, 1.0, 1.0],
    [0.96, 0.96, 0.96, 1.0],
    [0.9, 0.9, 0.9, 1.0],
    [0.86, 0.86, 0.86, 1.0],
];
const SEGMENT_BORDER: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
const SELECTED_SEGMENT_BORDER: [f32; 4] = [0.16, 0.29, 0.55, 1.0];

/// A plain segmented control, as on iPhone OS 3: a white rounded rectangle
/// split into `segments` equal parts, with the `selected` one blue.
pub(super) fn draw_segmented_control(
    env: &mut Environment,
    view: id,
    segments: usize,
    selected: Option<usize>,
) {
    let bounds: CGRect = msg![env; view bounds];
    let scale = draw_scale(env);
    let width = (bounds.size.width * scale).round() as u32;
    let height = (bounds.size.height * scale).round() as u32;
    if width == 0 || height == 0 || segments == 0 {
        return;
    }
    let full = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: width as f32,
            height: height as f32,
        },
    };
    let radius = 7.0 * scale;
    let mut canvas = Canvas::new(width, height);
    let border = Some((scale, SEGMENT_BORDER, SEGMENT_BORDER));
    canvas.rounded_rect(full, radius, border, |t| gloss_at(&PLAIN_SEGMENT, t));
    let segment_x = |i: usize| (width as usize * i / segments) as u32;
    if let Some(i) = selected {
        // The whole control in blue, of which only this segment is used, so
        // the end segments keep the rounded corners.
        let mut blue = Canvas::new(width, height);
        let border = Some((scale, SELECTED_SEGMENT_BORDER, SELECTED_SEGMENT_BORDER));
        blue.rounded_rect(full, radius, border, |t| gloss_at(&DONE_BUTTON, t));
        for y in 0..height {
            for x in segment_x(i)..segment_x(i + 1) {
                let p = (y * width + x) as usize;
                canvas.pixels[p] = blue.pixels[p];
            }
        }
    }
    for i in 1..segments {
        let divider = CGRect {
            origin: CGPoint {
                x: segment_x(i) as f32 - scale / 2.0,
                y: 0.0,
            },
            size: CGSize {
                width: scale,
                height: height as f32,
            },
        };
        canvas.rounded_rect(divider, 0.0, None, |_| SEGMENT_BORDER);
    }
    set_layer_image(env, view, canvas);
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIToolbar: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIToolbarHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (CGSize)sizeThatFits:(CGSize)size {
    // Tested with real iOS that height for Toolbar is 44.0
    CGSize { width: size.width, height: 44.0 }
}

- (id)initWithFrame:(CGRect)frame {
    let mut frame = frame;
    let current_frame_size = frame.size;
    let size = msg![env; this sizeThatFits:current_frame_size];
    frame.size = size;

    let this: id = msg_super![env; this initWithFrame:frame];
    () = msg![env; this setOpaque:false];
    () = msg![env; this setBarStyle:UIBarStyleDefault];
    this
}

- (())dealloc {
    let UIToolbarHostObject {
        superclass: _,
        items,
        item_views,
        bar_style: _,
    } = std::mem::take(env.objc.borrow_mut(this));

    for item in items {
        release(env, item);
    }
    for view in item_views {
        release(env, view);
    }

    msg_super![env; this dealloc]
}

- (())setItems:(id)items { // NSArray *
    msg![env; this setItems:items animated:false]
}

- (())setItems:(id)items // NSArray *
            animated:(bool)animated {
    if animated {
        log_dbg!("TODO: UIToolbar setItems:animated: animation not supported, ignoring");
    }

    let count: NSUInteger = msg![env; items count];
    let mut tmp_items: Vec<id> = Vec::new();
    let mut tmp_views: Vec<id> = Vec::new();

    for i in 0..count {
        let next: id = msg![env; items objectAtIndex:i];
        retain(env, next);
        tmp_items.push(next);
        let view: id = msg![env; next customView];
        retain(env, view);
        tmp_views.push(view);
    }

    // Remove old item views from superview
    let host = env.objc.borrow_mut::<UIToolbarHostObject>(this);
    let old_items = std::mem::replace(&mut host.items, tmp_items);
    let old_views = std::mem::replace(&mut host.item_views, tmp_views);

    for view in old_views {
        () = msg![env; view removeFromSuperview];
        release(env, view);
    }
    for item in old_items {
        release(env, item);
    }

    // Add new item views as subviews
    let item_views = env.objc.borrow::<UIToolbarHostObject>(this).item_views.to_vec();
    for view in item_views {
        () = msg![env; this addSubview:view];
    }

    () = msg![env; this layoutSubviews];
    () = msg![env; this setNeedsDisplay];
}

- (id)items {
    let vcs = env.objc.borrow::<UIToolbarHostObject>(this).items.to_vec();
    for vc in &vcs {
        retain(env, *vc);
    }
    let res = ns_array::from_vec(env, vcs);
    autorelease(env, res)
}

- (())setBarStyle:(UIBarStyle)style {
    match style {
        UIBarStyleDefault | UIBarStyleBlack | UIBarStyleBlackTranslucent => {}
        _ => unimplemented!("UIBarStyle {}", style),
    };
    env.objc.borrow_mut::<UIToolbarHostObject>(this).bar_style = style;

    // The background is drawn as an image (see draw_background).
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; this setBackgroundColor:clear];
    // Redraw the background and the buttons (not apps' custom views).
    let host = env.objc.borrow::<UIToolbarHostObject>(this);
    let (items, views) = (host.items.clone(), host.item_views.clone());
    let mut to_clear = vec![this];
    for (item, view) in items.into_iter().zip(views) {
        let label: id = msg![env; item label];
        if label != nil {
            to_clear.push(view);
        }
    }
    for view in to_clear {
        let layer: id = msg![env; view layer];
        () = msg![env; layer setContents:nil];
    }
    () = msg![env; this layoutSubviews];
}

- (UIBarStyle)barStyle {
    env.objc.borrow::<UIToolbarHostObject>(this).bar_style
}

- (())setTranslucent:(bool)translucent {
    log!("TODO: setTranslucent:{} ignored", translucent);
}

- (bool)isTranslucent {
    log!("TODO: isTranslucent returning false");
    false
}

- (())setFrame:(CGRect)frame {
    () = msg_super![env; this setFrame:frame];

    // UIView's addSubview: doesn't trigger a layout pass on the new subview,
    // so toolbars loaded from NIBs lay out once at zero width and never again.
    // Re-running layout on frame changes covers the case where the parent
    // sizes the toolbar after mounting it.
    () = msg![env; this layoutSubviews];
}

- (())layoutSubviews {
    let bar_style = env.objc.borrow::<UIToolbarHostObject>(this).bar_style;
    draw_bar_background(env, this, bar_style, true);
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UIToolbarHostObject>(this);
    let items = host.items.to_vec();
    let item_views = host.item_views.to_vec();
    let bar_style = host.bar_style;
    if items.is_empty() {
        return;
    }

    let mut total_fixed_width: CGFloat = 0.0;
    let mut item_widths: Vec<CGFloat> = Vec::new();
    let mut flexible_space_count: usize = 0;

    for &item in &items {
        let width: CGFloat = msg![env; item width];
        if width > 0.0 {
            item_widths.push(width);
            total_fixed_width += width;
        } else {
            let system_item = env.objc.borrow::<UIBarButtonItemHostObject>(item).system_item;
            if system_item == Some(UIBarButtonSystemItemFlexibleSpace) {
                flexible_space_count += 1;
                item_widths.push(0.0);
            } else if system_item == Some(UIBarButtonSystemItemFixedSpace) {
                let fixed_width = 42.0;
                item_widths.push(fixed_width);
                total_fixed_width += fixed_width;
            } else {
                let bounds_size = bounds.size;
                let size: CGSize = msg![env; item sizeThatFits:bounds_size];
                item_widths.push(size.width);
                total_fixed_width += size.width;
            }
        }
    }

    let item_count = items.len();
    let remaining_width = bounds.size.width - EDGE_INSET * 2.0 - total_fixed_width;

    // When no FlexibleSpace items are present, distribute slack between items
    // so the first/last items sit at the toolbar's edge insets. With a single
    // item, center it in the toolbar.
    let (flexible_width, gap, start_x) = if flexible_space_count > 0 {
        ((remaining_width / (flexible_space_count as f32)).max(0.0), 0.0, 0.0)
    } else if item_count > 1 {
        (0.0, (remaining_width / ((item_count - 1) as f32)).max(0.0), 0.0)
    } else {
        (0.0, 0.0, (remaining_width / 2.0).max(0.0))
    };

    let mut x = EDGE_INSET + start_x;
    let button_margin: CGFloat = 4.0;
    let h = (bounds.size.height - button_margin * 2.0).max(0.0);
    let y = button_margin;

    for (i, &item) in items.iter().enumerate() {
        let mut w = item_widths[i];
        if w == 0.0 {
            let system_item = env.objc.borrow::<UIBarButtonItemHostObject>(item).system_item;
            if system_item == Some(UIBarButtonSystemItemFlexibleSpace) {
                w = flexible_width;
            }
        }

        // Titled items other than plain ones are drawn as bordered buttons.
        let label: id = msg![env; item label];
        let item_style = env.objc.borrow::<UIBarButtonItemHostObject>(item).style;
        let bordered = label != nil && item_style != UIBarButtonItemStylePlain;
        let system_item = env.objc.borrow::<UIBarButtonItemHostObject>(item).system_item;
        let (y, h) = if bordered {
            ((bounds.size.height - BUTTON_HEIGHT) / 2.0, BUTTON_HEIGHT)
        } else if label == nil && system_item.is_none() {
            // An app's custom view keeps its height, centred.
            let view_bounds: CGRect = msg![env; (item_views[i]) bounds];
            let h = view_bounds.size.height;
            (((bounds.size.height - h) / 2.0).round(), h)
        } else {
            (y, h)
        };
        // Whole points, as in UIKit, so the button images aren't blurred.
        let frame = CGRect {
            origin: CGPoint { x: x.round(), y },
            size: CGSize {
                width: (x + w).round() - x.round(),
                height: h,
            },
        };

        // Position the item's internal view, then fill its label to bounds.
        let item_view = item_views[i];
        () = msg![env; item_view setFrame:frame];
        if label != nil {
            let view_bounds: CGRect = msg![env; item_view bounds];
            () = msg![env; label setFrame:view_bounds];
        }
        if bordered {
            draw_button(env, item_view, bar_style, item_style);
        }

        x += w + gap;
    }
}

@end

};
