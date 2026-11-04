/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIScreen`.

use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::objc::{
    autorelease, id, msg, msg_class, objc_classes, ClassExports, HostObject, TrivialHostObject,
};

#[derive(Default)]
pub struct State {
    main_screen: Option<id>,
}

struct UIScreenModeHostObject {
    /// In pixels.
    size: CGSize,
}
impl HostObject for UIScreenModeHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// For now this is a singleton (the only instance is returned by mainScreen),
// so there are hardcoded assumptions related to that.
@implementation UIScreen: NSObject

+ (id)mainScreen {
    if let Some(screen) = env.framework_state.uikit.ui_screen.main_screen {
        screen
    } else {
        let new = env.objc.alloc_static_object(
            this,
            Box::new(TrivialHostObject),
            &mut env.mem
        );
        env.framework_state.uikit.ui_screen.main_screen = Some(new);
        new
   }
}

// We only model just one screen (main screen).
+ (id)screens {
    let main_screen: id = msg![env; this mainScreen];
    msg_class![env; NSArray arrayWithObject:main_screen]
}

- (id)retain { this }
- (())release {}
- (id)autorelease { this }

// TODO: more accessors

- (CGRect)bounds {
    // While Apple's documentation says this changes with the interface
    // orientation, https://useyourloaf.com/blog/uiscreen-bounds-in-ios-8/ says
    // ths wasn't the case prior to iOS 8.
    let (width, height) = env.window().portrait_size();
    CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: width as f32, height: height as f32 },
    }
}

- (CGRect)applicationFrame {
    // FIXME: Does this change depending on the status bar orientation?
    let mut bounds: CGRect = msg![env; this bounds];
    const STATUS_BAR_HEIGHT: f32 = 20.0;
    if !env.framework_state.uikit.ui_application.status_bar_hidden {
        bounds.origin.y += STATUS_BAR_HEIGHT;
        bounds.size.height -= STATUS_BAR_HEIGHT;
    }
    bounds
}

- (CGFloat)scale {
    // TODO: support retina
    1.0
}

// The main screen has just one mode: its size in pixels.
- (id)currentMode {
    let bounds: CGRect = msg![env; this bounds];
    let scale: CGFloat = msg![env; this scale];
    let size = CGSize {
        width: bounds.size.width * scale,
        height: bounds.size.height * scale,
    };
    let class = env.objc.get_known_class("UIScreenMode", &mut env.mem);
    let mode = env.objc.alloc_object(class, Box::new(UIScreenModeHostObject { size }), &mut env.mem);
    autorelease(env, mode)
}
- (id)availableModes {
    let mode: id = msg![env; this currentMode];
    msg_class![env; NSArray arrayWithObject:mode]
}
- (())setCurrentMode:(id)mode { // UIScreenMode*
    let current: id = msg![env; this currentMode];
    let size: CGSize = msg![env; mode size];
    let current_size: CGSize = msg![env; current size];
    // There's no other mode to change to.
    assert!(size.width == current_size.width && size.height == current_size.height);
}

@end

@implementation UIScreenMode: NSObject

- (CGSize)size {
    env.objc.borrow::<UIScreenModeHostObject>(this).size
}
- (CGFloat)pixelAspectRatio {
    1.0
}
- (bool)isEqual:(id)other {
    let class: id = msg![env; this class];
    if !msg![env; other isKindOfClass:class] {
        return false;
    }
    let size: CGSize = msg![env; this size];
    let other_size: CGSize = msg![env; other size];
    size.width == other_size.width && size.height == other_size.height
}

@end

};
