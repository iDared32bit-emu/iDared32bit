/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableViewCell`.
//!
//! Only the default style: an optional image, then a single text label.
//! Accessories are remembered but not drawn yet.

use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    ClassExports, NSZonePtr,
};

pub type UITableViewCellStyle = NSInteger;
const UITableViewCellStyleDefault: UITableViewCellStyle = 0;

type UITableViewCellAccessoryType = NSInteger;
const UITableViewCellAccessoryNone: UITableViewCellAccessoryType = 0;

type UITableViewCellSelectionStyle = NSInteger;
const UITableViewCellSelectionStyleNone: UITableViewCellSelectionStyle = 0;
const UITableViewCellSelectionStyleBlue: UITableViewCellSelectionStyle = 1;

/// The default height of a row.
pub const DEFAULT_ROW_HEIGHT: CGFloat = 44.0;

pub struct UITableViewCellHostObject {
    superclass: super::UIViewHostObject,
    /// These views are strong references.
    content_view: id,
    text_label: id,
    /// Made when first asked for.
    image_view: id,
    background_view: id,
    selected_background_view: id,
    /// `NSString*`
    reuse_identifier: id,
    accessory_type: UITableViewCellAccessoryType,
    selection_style: UITableViewCellSelectionStyle,
    selected: bool,
    highlighted: bool,
}
impl_HostObject_with_superclass!(UITableViewCellHostObject);
impl Default for UITableViewCellHostObject {
    fn default() -> Self {
        UITableViewCellHostObject {
            superclass: Default::default(),
            content_view: nil,
            text_label: nil,
            image_view: nil,
            background_view: nil,
            selected_background_view: nil,
            reuse_identifier: nil,
            accessory_type: UITableViewCellAccessoryNone,
            selection_style: UITableViewCellSelectionStyleBlue,
            selected: false,
            highlighted: false,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableViewCell: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewCellHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithStyle:(UITableViewCellStyle)style
    reuseIdentifier:(id)reuse_identifier { // NSString*
    if style != UITableViewCellStyleDefault {
        log!("TODO: UITableViewCell style {}, using the default style", style);
    }
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 320.0, height: DEFAULT_ROW_HEIGHT },
    };
    msg![env; this initWithFrame:frame reuseIdentifier:reuse_identifier]
}

// Deprecated in iPhone OS 3.0, but older games use it.
- (id)initWithFrame:(CGRect)frame
    reuseIdentifier:(id)reuse_identifier { // NSString*
    let this: id = msg_super![env; this initWithFrame:frame];
    let reuse_identifier: id = msg![env; reuse_identifier copy];

    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; this setBackgroundColor:white];

    let bounds: CGRect = msg![env; this bounds];
    let selected_background_view: id = msg_class![env; UIView alloc];
    let selected_background_view: id =
        msg![env; selected_background_view initWithFrame:bounds];
    let blue: id = msg_class![env; UIColor colorWithRed:(0.02 as CGFloat)
                                                  green:(0.45 as CGFloat)
                                                   blue:(0.88 as CGFloat)
                                                  alpha:(1.0 as CGFloat)];
    () = msg![env; selected_background_view setBackgroundColor:blue];
    () = msg![env; selected_background_view setHidden:true];
    () = msg![env; this addSubview:selected_background_view];

    let content_view: id = msg_class![env; UIView alloc];
    let content_view: id = msg![env; content_view initWithFrame:bounds];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; content_view setBackgroundColor:clear];
    () = msg![env; this addSubview:content_view];

    let text_label: id = msg_class![env; UILabel new];
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:(20.0 as CGFloat)];
    () = msg![env; text_label setFont:font];
    () = msg![env; text_label setBackgroundColor:clear];
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; text_label setHighlightedTextColor:white];
    () = msg![env; content_view addSubview:text_label];

    let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
    host.content_view = content_view;
    host.text_label = text_label;
    host.selected_background_view = selected_background_view;
    host.reuse_identifier = reuse_identifier;
    this
}

- (())dealloc {
    let &UITableViewCellHostObject {
        content_view,
        text_label,
        image_view,
        background_view,
        selected_background_view,
        reuse_identifier,
        ..
    } = env.objc.borrow(this);
    release(env, content_view);
    release(env, text_label);
    release(env, image_view);
    release(env, background_view);
    release(env, selected_background_view);
    release(env, reuse_identifier);
    msg_super![env; this dealloc]
}

- (id)reuseIdentifier {
    env.objc.borrow::<UITableViewCellHostObject>(this).reuse_identifier
}
- (())prepareForReuse {
    () = msg![env; this setSelected:false animated:false];
}

- (id)contentView {
    env.objc.borrow::<UITableViewCellHostObject>(this).content_view
}
- (id)textLabel {
    env.objc.borrow::<UITableViewCellHostObject>(this).text_label
}
- (id)imageView {
    let image_view = env.objc.borrow::<UITableViewCellHostObject>(this).image_view;
    if image_view != nil {
        return image_view;
    }
    let image_view: id = msg_class![env; UIImageView new];
    let content_view: id = msg![env; this contentView];
    () = msg![env; content_view addSubview:image_view];
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).image_view = image_view;
    () = msg![env; this setNeedsLayout];
    image_view
}

- (id)backgroundView {
    env.objc.borrow::<UITableViewCellHostObject>(this).background_view
}
- (())setBackgroundView:(id)view { // UIView*
    let old = env.objc.borrow::<UITableViewCellHostObject>(this).background_view;
    if old == view {
        return;
    }
    () = msg![env; old removeFromSuperview];
    release(env, old);
    let view: id = msg![env; view retain];
    if view != nil {
        // Behind everything else.
        () = msg![env; this insertSubview:view atIndex:0i32];
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).background_view = view;
    () = msg![env; this setNeedsLayout];
}

- (id)selectedBackgroundView {
    env.objc.borrow::<UITableViewCellHostObject>(this).selected_background_view
}
- (())setSelectedBackgroundView:(id)view { // UIView*
    let old = env.objc.borrow::<UITableViewCellHostObject>(this).selected_background_view;
    if old == view {
        return;
    }
    () = msg![env; old removeFromSuperview];
    release(env, old);
    let view: id = msg![env; view retain];
    if view != nil {
        // Behind the content, in front of the background view.
        let content_view: id = msg![env; this contentView];
        () = msg![env; this insertSubview:view belowSubview:content_view];
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selected_background_view = view;
    () = msg![env; this _touchHLE_updateSelection];
}

// The deprecated text properties are the text label's.
- (id)text {
    let label: id = msg![env; this textLabel];
    msg![env; label text]
}
- (())setText:(id)text { // NSString*
    let label: id = msg![env; this textLabel];
    () = msg![env; label setText:text];
}
- (id)font {
    let label: id = msg![env; this textLabel];
    msg![env; label font]
}
- (())setFont:(id)font { // UIFont*
    let label: id = msg![env; this textLabel];
    () = msg![env; label setFont:font];
}
- (id)textColor {
    let label: id = msg![env; this textLabel];
    msg![env; label textColor]
}
- (())setTextColor:(id)color { // UIColor*
    let label: id = msg![env; this textLabel];
    () = msg![env; label setTextColor:color];
}
- (id)image {
    let image_view: id = msg![env; this imageView];
    msg![env; image_view image]
}
- (())setImage:(id)image { // UIImage*
    let image_view: id = msg![env; this imageView];
    () = msg![env; image_view setImage:image];
    () = msg![env; this setNeedsLayout];
}

- (UITableViewCellAccessoryType)accessoryType {
    env.objc.borrow::<UITableViewCellHostObject>(this).accessory_type
}
- (())setAccessoryType:(UITableViewCellAccessoryType)accessory_type {
    if accessory_type != UITableViewCellAccessoryNone {
        log!("TODO: UITableViewCell accessory {} isn't drawn", accessory_type);
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).accessory_type = accessory_type;
}

- (UITableViewCellSelectionStyle)selectionStyle {
    env.objc.borrow::<UITableViewCellHostObject>(this).selection_style
}
- (())setSelectionStyle:(UITableViewCellSelectionStyle)style {
    if style > UITableViewCellSelectionStyleBlue {
        log!("TODO: UITableViewCell selection style {}, using blue", style);
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selection_style = style;
    () = msg![env; this _touchHLE_updateSelection];
}

- (bool)isSelected {
    env.objc.borrow::<UITableViewCellHostObject>(this).selected
}
- (())setSelected:(bool)selected {
    () = msg![env; this setSelected:selected animated:false];
}
- (())setSelected:(bool)selected animated:(bool)_animated {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selected = selected;
    () = msg![env; this _touchHLE_updateSelection];
}
- (bool)isHighlighted {
    env.objc.borrow::<UITableViewCellHostObject>(this).highlighted
}
- (())setHighlighted:(bool)highlighted {
    () = msg![env; this setHighlighted:highlighted animated:false];
}
- (())setHighlighted:(bool)highlighted animated:(bool)_animated {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).highlighted = highlighted;
    () = msg![env; this _touchHLE_updateSelection];
}

// Shows the selected background and highlighted text while the cell is
// selected or highlighted.
- (())_touchHLE_updateSelection {
    let &UITableViewCellHostObject {
        text_label,
        selected_background_view,
        selection_style,
        selected,
        highlighted,
        ..
    } = env.objc.borrow(this);
    let shown = (selected || highlighted) && selection_style != UITableViewCellSelectionStyleNone;
    () = msg![env; selected_background_view setHidden:(!shown)];
    () = msg![env; text_label setHighlighted:shown];
}

- (())layoutSubviews {
    let bounds: CGRect = msg![env; this bounds];
    let &UITableViewCellHostObject {
        content_view,
        text_label,
        image_view,
        background_view,
        selected_background_view,
        ..
    } = env.objc.borrow(this);
    if background_view != nil {
        () = msg![env; background_view setFrame:bounds];
    }
    () = msg![env; selected_background_view setFrame:bounds];
    () = msg![env; content_view setFrame:bounds];

    // The image is at the left, centred vertically, then the text.
    let mut text_x: CGFloat = 10.0;
    if image_view != nil {
        let image: id = msg![env; image_view image];
        if image != nil {
            let size: CGSize = msg![env; image size];
            let frame = CGRect {
                origin: CGPoint {
                    x: 10.0,
                    y: ((bounds.size.height - size.height) / 2.0).round(),
                },
                size,
            };
            () = msg![env; image_view setFrame:frame];
            text_x += size.width + 10.0;
        }
    }
    let frame = CGRect {
        origin: CGPoint { x: text_x, y: 0.0 },
        size: CGSize {
            width: (bounds.size.width - text_x - 10.0).max(0.0),
            height: bounds.size.height,
        },
    };
    () = msg![env; text_label setFrame:frame];
}

@end

};
