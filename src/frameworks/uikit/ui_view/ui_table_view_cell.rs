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
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    ClassExports, NSZonePtr,
};
use crate::Environment;

pub type UITableViewCellStyle = NSInteger;
const UITableViewCellStyleDefault: UITableViewCellStyle = 0;

type UITableViewCellAccessoryType = NSInteger;
const UITableViewCellAccessoryNone: UITableViewCellAccessoryType = 0;

type UITableViewCellSelectionStyle = NSInteger;
const UITableViewCellSelectionStyleNone: UITableViewCellSelectionStyle = 0;
const UITableViewCellSelectionStyleBlue: UITableViewCellSelectionStyle = 1;

pub type UITableViewCellEditingStyle = NSInteger;
pub const UITableViewCellEditingStyleNone: UITableViewCellEditingStyle = 0;
pub const UITableViewCellEditingStyleDelete: UITableViewCellEditingStyle = 1;
pub const UITableViewCellEditingStyleInsert: UITableViewCellEditingStyle = 2;

/// The default height of a row.
pub const DEFAULT_ROW_HEIGHT: CGFloat = 44.0;

/// How far the content moves right while editing, to make room for the
/// delete or insert control. Taps left of this are on the control.
pub const EDITING_INDENT: CGFloat = 32.0;
const EDITING_CONTROL_SIZE: CGFloat = 22.0;
const DELETE_BUTTON_SIZE: CGSize = CGSize {
    width: 64.0,
    height: 30.0,
};
const DELETE_BUTTON_MARGIN: CGFloat = 8.0;

/// Where the Delete button is, in a cell's co-ordinates.
pub fn delete_button_rect(bounds: CGRect) -> CGRect {
    CGRect {
        origin: CGPoint {
            x: bounds.size.width - DELETE_BUTTON_SIZE.width - DELETE_BUTTON_MARGIN,
            y: ((bounds.size.height - DELETE_BUTTON_SIZE.height) / 2.0).round(),
        },
        size: DELETE_BUTTON_SIZE,
    }
}

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
    editing: bool,
    editing_style: UITableViewCellEditingStyle,
    showing_delete_confirmation: bool,
    /// The editing control (a circle with bars in it) and the Delete
    /// button, made when first needed. Strong references.
    editing_control: id,
    delete_button: id,
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
            editing: false,
            editing_style: UITableViewCellEditingStyleDelete,
            showing_delete_confirmation: false,
            editing_control: nil,
            delete_button: nil,
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
        editing_control,
        delete_button,
        ..
    } = env.objc.borrow(this);
    release(env, editing_control);
    release(env, delete_button);
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
    () = msg![env; this setHighlighted:false animated:false];
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

- (bool)isEditing {
    env.objc.borrow::<UITableViewCellHostObject>(this).editing
}
- (())setEditing:(bool)editing {
    () = msg![env; this setEditing:editing animated:false];
}
// Changes straight away, even when animated.
- (())setEditing:(bool)editing animated:(bool)_animated {
    let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
    if host.editing == editing {
        return;
    }
    host.editing = editing;
    () = msg![env; this setNeedsLayout];
}
- (UITableViewCellEditingStyle)editingStyle {
    env.objc.borrow::<UITableViewCellHostObject>(this).editing_style
}
- (bool)showingDeleteConfirmation {
    env.objc.borrow::<UITableViewCellHostObject>(this).showing_delete_confirmation
}
// The table view decides these.
- (())_touchHLE_setEditingStyle:(UITableViewCellEditingStyle)style {
    let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
    if host.editing_style == style {
        return;
    }
    host.editing_style = style;
    () = msg![env; this setNeedsLayout];
}
- (())_touchHLE_setShowingDeleteConfirmation:(bool)showing {
    let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
    if host.showing_delete_confirmation == showing {
        return;
    }
    host.showing_delete_confirmation = showing;
    () = msg![env; this setNeedsLayout];
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

    // While editing, the content moves over for the editing control.
    let editing = env.objc.borrow::<UITableViewCellHostObject>(this).editing;
    let indent = if editing { EDITING_INDENT } else { 0.0 };
    let content_frame = CGRect {
        origin: CGPoint {
            x: bounds.origin.x + indent,
            y: bounds.origin.y,
        },
        size: CGSize {
            width: (bounds.size.width - indent).max(0.0),
            height: bounds.size.height,
        },
    };
    () = msg![env; content_view setFrame:content_frame];
    layout_editing_views(env, this, bounds);

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
            width: (content_frame.size.width - text_x - 10.0).max(0.0),
            height: bounds.size.height,
        },
    };
    () = msg![env; text_label setFrame:frame];
}

@end

};

fn color(env: &mut Environment, r: CGFloat, g: CGFloat, b: CGFloat) -> id {
    msg_class![env; UIColor colorWithRed:r green:g blue:b alpha:(1.0 as CGFloat)]
}

/// A plain view that taps go through (to the table view).
fn make_view(env: &mut Environment, frame: CGRect, background: id) -> id {
    let view: id = msg_class![env; UIView alloc];
    let view: id = msg![env; view initWithFrame:frame];
    () = msg![env; view setBackgroundColor:background];
    () = msg![env; view setUserInteractionEnabled:false];
    view
}

/// The editing control: a red circle with a minus to delete (turned upright
/// while the Delete button shows), or a green one with a plus to insert.
fn make_editing_control(env: &mut Environment) -> id {
    let size = EDITING_CONTROL_SIZE;
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: size,
            height: size,
        },
    };
    let control = make_view(env, frame, nil);
    let layer: id = msg![env; control layer];
    () = msg![env; layer setCornerRadius:(size / 2.0)];
    let white: id = msg_class![env; UIColor whiteColor];
    let (long, short) = (12.0, 3.0);
    for (tag, width, height) in [(1, long, short), (2, short, long)] {
        let bar_frame = CGRect {
            origin: CGPoint {
                x: (size - width) / 2.0,
                y: (size - height) / 2.0,
            },
            size: CGSize { width, height },
        };
        let bar = make_view(env, bar_frame, white);
        () = msg![env; bar setTag:(tag as NSInteger)];
        () = msg![env; control addSubview:bar];
        release(env, bar);
    }
    control
}

fn make_delete_button(env: &mut Environment) -> id {
    let red = color(env, 0.86, 0.13, 0.13);
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: DELETE_BUTTON_SIZE,
    };
    let button = make_view(env, frame, red);
    let layer: id = msg![env; button layer];
    () = msg![env; layer setCornerRadius:(6.0 as CGFloat)];
    let label: id = msg_class![env; UILabel alloc];
    let label: id = msg![env; label initWithFrame:frame];
    let title = get_static_str(env, "Delete");
    () = msg![env; label setText:title];
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:(15.0 as CGFloat)];
    () = msg![env; label setFont:font];
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; label setTextColor:white];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; label setBackgroundColor:clear];
    () = msg![env; label setTextAlignment:UITextAlignmentCenter];
    () = msg![env; button addSubview:label];
    release(env, label);
    button
}

fn layout_editing_views(env: &mut Environment, this: id, bounds: CGRect) {
    let &UITableViewCellHostObject {
        editing,
        editing_style,
        showing_delete_confirmation,
        mut editing_control,
        mut delete_button,
        ..
    } = env.objc.borrow(this);

    let show_control = editing && editing_style != UITableViewCellEditingStyleNone;
    if show_control && editing_control == nil {
        editing_control = make_editing_control(env);
        () = msg![env; this addSubview:editing_control];
        env.objc
            .borrow_mut::<UITableViewCellHostObject>(this)
            .editing_control = editing_control;
    }
    if editing_control != nil {
        () = msg![env; editing_control setHidden:(!show_control)];
        let insert = editing_style == UITableViewCellEditingStyleInsert;
        let background = if insert {
            color(env, 0.2, 0.7, 0.2)
        } else {
            color(env, 0.86, 0.13, 0.13)
        };
        () = msg![env; editing_control setBackgroundColor:background];
        let horizontal: id = msg![env; editing_control viewWithTag:(1 as NSInteger)];
        let vertical: id = msg![env; editing_control viewWithTag:(2 as NSInteger)];
        () = msg![env; horizontal setHidden:(!insert && showing_delete_confirmation)];
        () = msg![env; vertical setHidden:(!insert && !showing_delete_confirmation)];
        let frame = CGRect {
            origin: CGPoint {
                x: ((EDITING_INDENT - EDITING_CONTROL_SIZE) / 2.0).round(),
                y: ((bounds.size.height - EDITING_CONTROL_SIZE) / 2.0).round(),
            },
            size: CGSize {
                width: EDITING_CONTROL_SIZE,
                height: EDITING_CONTROL_SIZE,
            },
        };
        () = msg![env; editing_control setFrame:frame];
        () = msg![env; this bringSubviewToFront:editing_control];
    }

    if showing_delete_confirmation && delete_button == nil {
        delete_button = make_delete_button(env);
        () = msg![env; this addSubview:delete_button];
        env.objc
            .borrow_mut::<UITableViewCellHostObject>(this)
            .delete_button = delete_button;
    }
    if delete_button != nil {
        () = msg![env; delete_button setHidden:(!showing_delete_confirmation)];
        () = msg![env; delete_button setFrame:(delete_button_rect(bounds))];
        () = msg![env; this bringSubviewToFront:delete_button];
    }
}
