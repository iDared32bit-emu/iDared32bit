/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Abstraction of window setup, OpenGL context creation and event handling.
//!
//! Implemented using the sdl2 crate (a Rust wrapper for SDL2). All usage of
//! SDL should be confined to this module.
//!
//! There is currently no separation of concerns between a single window and
//! window system interaction in general, because it is assumed only one window
//! will be needed for the runtime of the app.

use crate::gles::present::{present_frame, HomeButtonOverlay};
use crate::gles::{create_gles1_ctx_no_parent_stack, GLESContext, GLES};
use crate::image::Image;
use crate::matrix::Matrix;
use crate::options::Options;
use crate::Environment;
use sdl2::mouse::MouseButton;
use sdl2::pixels::PixelFormatEnum;
use sdl2::surface::Surface;
use sdl2_sys::SDL_PowerState;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::f32::consts::{FRAC_PI_2, PI};
use std::num::NonZeroU32;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

#[repr(C)]
#[allow(non_camel_case_types)]
#[cfg(target_os = "ios")]
struct SDL_SysWMinfoUIKit {
    _window: *const std::ffi::c_void,
    framebuffer: std::ffi::c_uint,
    colorbuffer: std::ffi::c_uint,
    resolve_framebuffer: std::ffi::c_uint,
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DeviceFamily {
    iPhone,
    iPad,
}
impl std::fmt::Display for DeviceFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
impl DeviceFamily {
    pub fn portrait_size(&self) -> (u32, u32) {
        match self {
            DeviceFamily::iPhone => (320, 480),
            DeviceFamily::iPad => (768, 1024),
        }
    }
}
impl TryFrom<u64> for DeviceFamily {
    type Error = ();
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(DeviceFamily::iPhone),
            2 => Ok(DeviceFamily::iPad),
            _ => Err(()),
        }
    }
}
impl TryFrom<&str> for DeviceFamily {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "iphone" => Ok(DeviceFamily::iPhone),
            "ipad" => Ok(DeviceFamily::iPad),
            _ => Err(()),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DeviceOrientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}
fn size_for_orientation(
    (width, height): (u32, u32),
    orientation: DeviceOrientation,
    scale_hack: NonZeroU32,
) -> (u32, u32) {
    let scale_hack = scale_hack.get();
    match orientation {
        DeviceOrientation::Portrait => (width * scale_hack, height * scale_hack),
        DeviceOrientation::PortraitUpsideDown => (width * scale_hack, height * scale_hack),
        DeviceOrientation::LandscapeLeft => (height * scale_hack, width * scale_hack),
        DeviceOrientation::LandscapeRight => (height * scale_hack, width * scale_hack),
    }
}
fn rotate_fullscreen_size(orientation: DeviceOrientation, screen_size: (u32, u32)) -> (u32, u32) {
    let (short_side, long_side) = if screen_size.0 < screen_size.1 {
        (screen_size.0, screen_size.1)
    } else {
        (screen_size.1, screen_size.0)
    };
    match orientation {
        DeviceOrientation::Portrait | DeviceOrientation::PortraitUpsideDown => {
            (short_side, long_side)
        }
        DeviceOrientation::LandscapeLeft | DeviceOrientation::LandscapeRight => {
            (long_side, short_side)
        }
    }
}
/// Tell SDL2 what orientation we want. Only useful on Android.
fn set_sdl2_orientation(orientation: DeviceOrientation) {
    // Despite the name, this hint works on Android too.
    sdl2::hint::set(
        "SDL_IOS_ORIENTATIONS",
        match orientation {
            DeviceOrientation::Portrait => "Portrait",
            // The inversion is deliberate. These probably correspond to
            // iPhone OS content orientations?
            DeviceOrientation::PortraitUpsideDown => "PortraitUpsideDown",
            DeviceOrientation::LandscapeLeft => "LandscapeRight",
            DeviceOrientation::LandscapeRight => "LandscapeLeft",
        },
    );
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum FingerId {
    Mouse,
    Touch(i64),
    VirtualCursor,
    ButtonToTouch(crate::options::Button),
    StickToTouch,
    DpadToTouch,
}
pub type Coords = (f32, f32);

/// A touch on the on-screen Home button (see [Window::home_button_overlay]).
/// Its later movement and release are the button's too, not the app's.
struct HomeButtonPress {
    finger_id: i64,
    /// A [Event::HomeButton] has been queued for it.
    triggered: bool,
}

#[derive(Clone, Copy)]
enum TouchKind {
    Down,
    Move,
    Up,
}

struct DpadState {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    active: bool,
}

#[derive(Debug)]
pub enum TextInputEvent {
    Text(String),
    Backspace,
    Return,
}

#[derive(Debug)]
pub enum Event {
    /// User requested quit.
    Quit,
    /// OS has informed touchHLE it will soon become inactive.
    /// (iOS `applicationWillResignActive:`, Android `onPause()`)
    AppWillResignActive,
    /// OS has informed touchHLE it will soon terminate.
    /// (iOS `applicationWillTerminate:`, Android `onDestroy()`)
    AppWillTerminate,
    TouchesDown(HashMap<FingerId, Coords>),
    TouchesMove(HashMap<FingerId, Coords>),
    TouchesUp(HashMap<FingerId, Coords>),
    /// User pressed F12, requesting that execution be paused and the debugger
    /// take over.
    EnterDebugger,
    /// User pressed Esc (on desktop) or Back (on Android), held a game
    /// controller's Back button, or touched the on-screen Home button (on
    /// iOS).
    HomeButton,
    TextInput(TextInputEvent),
}

pub enum BatteryState {
    Unknown,
    OnBattery,
    NoBattery,
    Charging,
    Full,
}

pub enum GLVersion {
    /// OpenGL ES 1.1
    GLES11,
    /// OpenGL 2.1 compatibility profile
    GL21Compat,
}

pub struct GLContext(sdl2::video::GLContext);

impl GLContext {
    pub fn is_current(&self) -> bool {
        self.0.is_current()
    }
}

fn surface_from_image(image: &Image) -> Surface<'_> {
    let src_pixels = image.pixels();
    let (width, height) = image.dimensions();

    let mut surface = Surface::new(width, height, PixelFormatEnum::RGBA32).unwrap();
    let (width, height) = (width as usize, height as usize);
    let pitch = surface.pitch() as usize;
    surface.with_lock_mut(|dst_pixels| {
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    let src_idx = y * width * 4 + x * 4 + channel;
                    let dst_idx = y * pitch + x * 4 + channel;
                    dst_pixels[dst_idx] = src_pixels[src_idx];
                }
            }
        }
    });
    surface
}

pub struct Window {
    _sdl_ctx: sdl2::Sdl,
    video_ctx: sdl2::VideoSubsystem,
    window: sdl2::video::Window,
    event_pump: sdl2::EventPump,
    event_queue: VecDeque<Event>,
    last_polled: Instant,
    /// Separate queue for extremely high-priority events (e.g. app about to
    /// terminate).
    high_priority_event: Option<Event>,
    enable_event_polling: bool,
    #[cfg(target_os = "macos")]
    max_height: u32,
    #[cfg(target_os = "macos")]
    viewport_y_offset: u32,
    /// Copy of `fullscreen` on [Options]. Note that this is meaningless when
    /// [Self::rotatable_fullscreen] returns [true].
    fullscreen: bool,
    scale_hack: NonZeroU32,
    internal_gl_ins: Option<Box<dyn GLESContext>>,
    splash_image: Option<Image>,
    device_family: DeviceFamily,
    /// The emulated screen's size in portrait, if it's not the device family's
    /// (see [Self::fit_screen_to_device]).
    screen_size: Option<(u32, u32)>,
    /// The emulated screen's safe area insets (top, left, bottom, right) in
    /// its own points, e.g. to keep clear of the Dynamic Island.
    screen_safe_insets: (f32, f32, f32, f32),
    /// The emulated screen's points per device point.
    screen_scale: f32,
    /// See [Self::enable_hidpi_ui].
    hidpi_ui: bool,
    device_orientation: DeviceOrientation,
    controller_ctx: sdl2::GameControllerSubsystem,
    controllers: Vec<sdl2::controller::GameController>,
    dpad_state: DpadState,
    stick_active: bool,
    _sensor_ctx: sdl2::SensorSubsystem,
    accelerometer: Option<sdl2::sensor::Sensor>,
    virtual_cursor_last: Option<(f32, f32, bool, bool)>,
    virtual_cursor_last_unsticky: Option<(f32, f32, Instant)>,
    virtual_accelerometer_last: Option<(f32, f32, bool)>,
    /// When a game controller's Back button was pressed, if it's still held
    /// and hasn't yet been held long enough to return to the app picker.
    controller_back_held_since: Option<Instant>,
    /// Whether to show the on-screen Home button, see
    /// [Self::enable_on_screen_home_button].
    on_screen_home_button: bool,
    home_button_press: Option<HomeButtonPress>,
    /// Whether or not we are on the "main" environment stack (rather than
    /// a coroutine stack). Checked in various functions to make sure that
    /// certain SDL functions (that call JNI functions) are on the main
    /// stack on Android.
    pub(super) on_main_stack: bool,
}

/// How long a game controller's Back button must be held to return to the app
/// picker. Holding avoids accidents in games that use it.
const CONTROLLER_BACK_HOLD: Duration = Duration::from_secs(1);

/// Whether this platform gets the on-screen Home button. Only iOS needs it,
/// since it has no Esc key or Back button.
const ON_SCREEN_HOME_BUTTON_PLATFORM: bool = cfg!(target_os = "ios");
/// The on-screen Home button's radius in points, if there's room for it.
const HOME_BUTTON_RADIUS: f32 = 22.0;
/// Its smallest radius in points; if there's no room for that, it's not shown.
const HOME_BUTTON_MIN_RADIUS: f32 = 12.0;
/// Its largest radius relative to the width of the bar it's in.
const HOME_BUTTON_BAR_RATIO: f32 = 0.35;

impl Window {
    /// Returns [true] if touchHLE is running on a device where we should always
    /// display fullscreen, but SDL2 will let us control the orientation, i.e.
    /// Android devices.
    pub fn rotatable_fullscreen() -> bool {
        env::consts::OS == "android" || env::consts::OS == "ios" || env::consts::OS == "ios-sim"
    }
    pub fn new(
        title: &str,
        icon: Option<Image>,
        launch_image: Option<Image>,
        options: &Options,
    ) -> Window {
        let sdl_ctx = sdl2::init().unwrap();
        let video_ctx = sdl_ctx.video().unwrap();

        // The "hidapi" feature of rust-sdl2 is enabled so that sdl2::sensor
        // is available, but we don't want to enable SDL's HIDAPI controller
        // drivers because they cause duplicated controllers on macOS
        // (https://github.com/libsdl-org/SDL/issues/7479). Once that's fixed,
        // remove this (https://github.com/touchHLE/touchHLE/issues/85).
        sdl2::hint::set("SDL_JOYSTICK_HIDAPI", "0");

        if env::consts::OS == "android" {
            // It's important to set context version BEFORE window creation
            // ref. https://wiki.libsdl.org/SDL2/SDL_GLattr
            let attr = video_ctx.gl_attr();
            attr.set_context_version(1, 1);
            attr.set_context_profile(sdl2::video::GLProfile::GLES);

            // Disable blocking of event loop when app is paused.
            sdl2::hint::set("SDL_ANDROID_BLOCK_ON_PAUSE", "0");
        }

        // Separate mouse and touch events
        sdl2::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");

        // SDL2 disables the screen saver by default, but iPhone OS enables
        // the idle timer that triggers sleep by default, so we turn it back on
        // here, and then the app can disable it if it wants to.
        video_ctx.enable_screen_saver();

        let scale_hack = options.scale_hack;
        // TODO: some apps specify their orientation in Info.plist, we could use
        // that here.
        let device_family = options.device_family.unwrap_or(DeviceFamily::iPhone);
        let device_orientation = options.initial_orientation;
        let fullscreen = options.fullscreen;

        let mut window = if Self::rotatable_fullscreen() {
            // Without this, SDL will force fullscreen mode to be portrait.
            set_sdl2_orientation(device_orientation);
            let screen_size = video_ctx.display_bounds(0).unwrap().size();
            let (width, height) = rotate_fullscreen_size(device_orientation, screen_size);
            let window = video_ctx
                .window(title, width, height)
                .fullscreen()
                .opengl()
                .build()
                .unwrap();
            window
        } else if fullscreen {
            let (width, height) = video_ctx.display_bounds(0).unwrap().size();
            let window = video_ctx
                .window(title, width, height)
                .fullscreen_desktop()
                .opengl()
                .build()
                .unwrap();
            window
        } else {
            let (width, height) = size_for_orientation(
                device_family.portrait_size(),
                device_orientation,
                scale_hack,
            );
            let window = video_ctx
                .window(title, width, height)
                .position_centered()
                .opengl()
                .build()
                .unwrap();
            window
        };

        if env::consts::OS == "android" {
            // Sanity check
            let gl_attr = video_ctx.gl_attr();
            debug_assert_eq!(gl_attr.context_profile(), sdl2::video::GLProfile::GLES);
            debug_assert_eq!(gl_attr.context_version(), (1, 1));
        }

        if let Some(icon) = icon {
            window.set_icon(surface_from_image(&icon));
        }

        let event_pump = sdl_ctx.event_pump().unwrap();

        let controller_ctx = sdl_ctx.game_controller().unwrap();

        let sensor_ctx = sdl_ctx.sensor().unwrap();
        let mut accelerometer: Option<sdl2::sensor::Sensor> = None;
        if let Ok(num_sensors) = sensor_ctx.num_sensors() {
            for sensor_idx in 0..num_sensors {
                if let Ok(sensor) = sensor_ctx.open(sensor_idx) {
                    if sensor.sensor_type() == sdl2::sensor::SensorType::Accelerometer {
                        log!("Accelerometer detected: {}.", sensor.name());
                        accelerometer = Some(sensor);
                        break;
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        let max_height = window.size().1;

        let mut window = Window {
            _sdl_ctx: sdl_ctx,
            video_ctx,
            window,
            event_pump,
            event_queue: VecDeque::new(),
            last_polled: Instant::now() - Duration::from_secs(1),
            high_priority_event: None,
            enable_event_polling: true,
            #[cfg(target_os = "macos")]
            max_height,
            #[cfg(target_os = "macos")]
            viewport_y_offset: 0,
            fullscreen,
            scale_hack,
            internal_gl_ins: None,
            splash_image: launch_image,
            device_family,
            screen_size: None,
            screen_safe_insets: (0.0, 0.0, 0.0, 0.0),
            screen_scale: 1.0,
            hidpi_ui: false,
            device_orientation,
            controller_ctx,
            controllers: Vec::new(),
            dpad_state: DpadState {
                left: false,
                right: false,
                up: false,
                down: false,
                active: false,
            },
            stick_active: false,
            _sensor_ctx: sensor_ctx,
            accelerometer,
            virtual_cursor_last: None,
            virtual_cursor_last_unsticky: None,
            virtual_accelerometer_last: None,
            controller_back_held_since: None,
            on_screen_home_button: false,
            home_button_press: None,
            on_main_stack: true,
        };

        // Set up OpenGL ES context used for splash screen and app UI rendering
        // (see src/frameworks/core_animation/composition.rs). OpenGL ES is used
        // because SDL2 won't let us use more than one graphics API in the same
        // window, and we also need OpenGL ES for the app's own rendering.
        let mut gl_ins = create_gles1_ctx_no_parent_stack(&mut window, options);
        {
            let gl_ctx = gl_ins.make_current(&mut window);
            log!("Driver info: {}", unsafe { gl_ctx.driver_description() });
        }
        window.internal_gl_ins = Some(gl_ins);

        if window.splash_image.is_some() {
            window.display_splash();
        }

        window
    }

    /// Poll for events from the OS. This needs to be done reasonably often
    /// (60Hz is probably fine) so that the host OS doesn't consider touchHLE
    /// to be unresponsive. Note that events are not returned by this function,
    /// since we often need to defer actually handling them.
    ///
    /// Since polling can be quite expensive, this function will skip it if it
    /// was called too recently.
    pub fn poll_for_events(&mut self, options: &Options) {
        assert!(self.on_main_stack);
        let now = Instant::now();
        // poll roughly twice per frame to try to avoid missing frames sometimes
        if now.duration_since(self.last_polled) < Duration::from_secs_f64(1.0 / 120.0) {
            return;
        }
        self.last_polled = now;

        fn transform_input_coords(
            window: &Window,
            (in_x, in_y): (f32, f32),
            independent_of_viewport: bool,
        ) -> (f32, f32) {
            let (vx, vy, vw, vh) = if independent_of_viewport {
                let (width, height) = size_for_orientation(
                    window.portrait_size(),
                    window.device_orientation,
                    NonZeroU32::new(1).unwrap(),
                );
                (0, 0, width, height)
            } else {
                window.viewport()
            };
            // normalize to unit square centred on origin
            let x = (in_x - vx as f32) / vw as f32 - 0.5;
            let y = (in_y - vy as f32) / vh as f32 - 0.5;
            // rotate
            let matrix = window.rotation_matrix().inverse().unwrap();
            let [x, y] = matrix.transform([x, y]);
            // back to pixels
            let (out_w, out_h) = window.size_unrotated_unscaled();
            let out_x = (x + 0.5) * out_w as f32;
            let out_y = (y + 0.5) * out_h as f32;
            // Round to match touch precision of official devices.
            (out_x.round(), out_y.round())
        }
        fn transform_virt_accel_coords(window: &Window, (in_x, in_y): (i32, i32)) -> (f32, f32) {
            let (_, _, vw, vh) = window.viewport();
            let out_x = ((in_x as f32 / vw as f32) * 2.0 - 1.0).clamp(-1.0, 1.0);
            let out_y = ((in_y as f32 / vh as f32) * 2.0 - 1.0).clamp(-1.0, 1.0);
            (out_x, out_y)
        }
        fn translate_button(button: sdl2::controller::Button) -> Option<crate::options::Button> {
            match button {
                sdl2::controller::Button::DPadLeft => Some(crate::options::Button::DPadLeft),
                sdl2::controller::Button::DPadUp => Some(crate::options::Button::DPadUp),
                sdl2::controller::Button::DPadRight => Some(crate::options::Button::DPadRight),
                sdl2::controller::Button::DPadDown => Some(crate::options::Button::DPadDown),
                sdl2::controller::Button::Start => Some(crate::options::Button::Start),
                sdl2::controller::Button::A => Some(crate::options::Button::A),
                sdl2::controller::Button::B => Some(crate::options::Button::B),
                sdl2::controller::Button::X => Some(crate::options::Button::X),
                sdl2::controller::Button::Y => Some(crate::options::Button::Y),
                sdl2::controller::Button::LeftShoulder => {
                    Some(crate::options::Button::LeftShoulder)
                }
                _ => None,
            }
        }
        fn finger_absolute_coords(window: &Window, (x, y): (f32, f32)) -> (f32, f32) {
            let (screen_width, screen_height) = window.window.drawable_size();
            (screen_width as f32 * x, screen_height as f32 * y)
        }

        let mut controller_updated = false;
        // event_pump doesn't have a method to peek on events
        // so, we keep track of an unconsumed one from a previous loop iteration
        // FIXME: use peek_event() from even_subsystem
        let mut previous_event: Option<sdl2::event::Event> = None;
        while self.enable_event_polling {
            use sdl2::event::Event as E;
            let event = if let Some(e) = previous_event.take() {
                match e {
                    E::Unknown { .. } => (),
                    _ => log_dbg!("Consuming previous event: {:?}", e),
                }
                e
            } else if let Some(e) = self.event_pump.poll_event() {
                match e {
                    E::Unknown { .. } => (),
                    _ => log_dbg!("Consuming new event: {:?}", e),
                }
                e
            } else {
                break;
            };

            // Virtual accelerometer
            match event {
                E::MouseButtonDown {
                    x,
                    y,
                    mouse_btn: MouseButton::Right,
                    ..
                } => {
                    let (x, y) = transform_virt_accel_coords(self, (x, y));
                    self.virtual_accelerometer_last = Some((x, y, true));
                }
                E::MouseMotion {
                    x, y, mousestate, ..
                } if mousestate.right() => {
                    let (x, y) = transform_virt_accel_coords(self, (x, y));
                    self.virtual_accelerometer_last = Some((x, y, true));
                }
                E::MouseButtonUp {
                    x,
                    y,
                    mouse_btn: MouseButton::Right,
                    ..
                } => {
                    let (x, y) = transform_virt_accel_coords(self, (x, y));
                    self.virtual_accelerometer_last = Some((x, y, false));
                }
                _ => {}
            }

            self.event_queue.push_back(match event {
                E::Quit { .. } => Event::Quit,
                E::MouseButtonDown {
                    x,
                    y,
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    let coords = transform_input_coords(self, (x as f32, y as f32), false);
                    log_dbg!("MouseButtonDown x {}, y {}, coords {:?}", x, y, coords);
                    Event::TouchesDown(HashMap::from([(FingerId::Mouse, coords)]))
                }
                E::MouseMotion {
                    x, y, mousestate, ..
                } if mousestate.left() => {
                    let coords = transform_input_coords(self, (x as f32, y as f32), false);
                    log_dbg!("MouseMotion x {}, y {}, coords {:?}", x, y, coords);
                    Event::TouchesMove(HashMap::from([(FingerId::Mouse, coords)]))
                }
                E::MouseButtonUp {
                    x,
                    y,
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    let coords = transform_input_coords(self, (x as f32, y as f32), false);
                    log_dbg!("MouseButtonUp x {}, y {}, coords {:?}", x, y, coords);
                    Event::TouchesUp(HashMap::from([(FingerId::Mouse, coords)]))
                }
                E::ControllerDeviceAdded { which, .. } => {
                    self.controller_added(which);
                    continue;
                }
                E::ControllerDeviceRemoved { which, .. } => {
                    self.controller_removed(which);
                    continue;
                }
                // Note that accelerometer simulation with analog sticks is
                // handled with polling, rather than being event-based.
                E::ControllerButtonUp { button, .. } | E::ControllerButtonDown { button, .. } => {
                    controller_updated = true;
                    // Holding Back returns to the app picker (see below).
                    if button == sdl2::controller::Button::Back {
                        self.controller_back_held_since =
                            matches!(event, E::ControllerButtonDown { .. }).then(Instant::now);
                        continue;
                    }
                    let Some(button) = translate_button(button) else {
                        continue;
                    };
                    // Called whenever a DPad direction is pressed or released
                    if (button == crate::options::Button::DPadLeft
                        || button == crate::options::Button::DPadUp
                        || button == crate::options::Button::DPadRight
                        || button == crate::options::Button::DPadDown)
                        && options.dpad_to_touch.is_some()
                    {
                        let Some((x, y, w, h)) = options.dpad_to_touch else {
                            unreachable!();
                        };

                        // Update held state
                        let pressed = matches!(event, E::ControllerButtonDown { .. });
                        match button {
                            crate::options::Button::DPadLeft => self.dpad_state.left = pressed,
                            crate::options::Button::DPadRight => self.dpad_state.right = pressed,
                            crate::options::Button::DPadUp => self.dpad_state.up = pressed,
                            crate::options::Button::DPadDown => self.dpad_state.down = pressed,
                            _ => unreachable!(),
                        }

                        // Compute center
                        let cx = x + w * 0.5;
                        let cy = y + h * 0.5;

                        // Compute combined delta
                        let mut dx = 0.0;
                        let mut dy = 0.0;

                        if self.dpad_state.left {
                            dx -= 0.5 * w;
                        }
                        if self.dpad_state.right {
                            dx += 0.5 * w;
                        }
                        if self.dpad_state.up {
                            dy -= 0.5 * h;
                        }
                        if self.dpad_state.down {
                            dy += 0.5 * h;
                        }

                        // Final coords: center + movement
                        let coords = transform_input_coords(self, (cx + dx, cy + dy), true);

                        // Send TouchDown if any dpad is held, TouchUp if none
                        let any_held = self.dpad_state.left
                            || self.dpad_state.right
                            || self.dpad_state.up
                            || self.dpad_state.down;

                        if !self.dpad_state.active && any_held {
                            // New touch
                            self.dpad_state.active = true;
                            Event::TouchesDown(HashMap::from([(FingerId::DpadToTouch, coords)]))
                        } else if self.dpad_state.active && any_held {
                            // Move existing touch
                            Event::TouchesMove(HashMap::from([(FingerId::DpadToTouch, coords)]))
                        } else if self.dpad_state.active && !any_held {
                            // Release touch
                            self.dpad_state.active = false;
                            Event::TouchesUp(HashMap::from([(FingerId::DpadToTouch, coords)]))
                        } else {
                            continue;
                        }
                    } else {
                        let Some(&(x, y)) = options.button_to_touch.get(&button) else {
                            continue;
                        };
                        match event {
                            E::ControllerButtonUp { .. } => {
                                let coords = transform_input_coords(self, (x, y), true);
                                Event::TouchesUp(HashMap::from([(
                                    FingerId::ButtonToTouch(button),
                                    coords,
                                )]))
                            }
                            E::ControllerButtonDown { .. } => {
                                let coords = transform_input_coords(self, (x, y), true);
                                Event::TouchesDown(HashMap::from([(
                                    FingerId::ButtonToTouch(button),
                                    coords,
                                )]))
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                E::ControllerAxisMotion { axis, .. } => {
                    controller_updated = true;
                    let Some((x, y, w, h)) = options.stick_to_touch else {
                        continue;
                    };
                    if axis == sdl2::controller::Axis::LeftX
                        || axis == sdl2::controller::Axis::LeftY
                    {
                        let (stick_x, stick_y, _) = self.get_controller_stick(options, true);
                        let coords = transform_input_coords(
                            self,
                            (
                                x + ((stick_x + 1.0) / 2.0) * w,
                                y + ((stick_y + 1.0) / 2.0) * h,
                            ),
                            true,
                        );
                        if stick_x.abs() < options.deadzone && stick_y.abs() < options.deadzone {
                            if !self.stick_active {
                                // Ignore deadzone events when stick is inactive
                                continue;
                            } else {
                                // Release touch when stick returns to deadzone
                                self.stick_active = false;
                                Event::TouchesUp(HashMap::from([(FingerId::StickToTouch, coords)]))
                            }
                        } else if !self.stick_active {
                            // New touch
                            self.stick_active = true;
                            Event::TouchesDown(HashMap::from([(FingerId::StickToTouch, coords)]))
                        } else {
                            // Move existing touch
                            Event::TouchesMove(HashMap::from([(FingerId::StickToTouch, coords)]))
                        }
                    } else {
                        continue;
                    }
                }
                E::AppWillEnterBackground { .. } => {
                    log!("Received app-will-resign-active event.");
                    assert!(self.high_priority_event.is_none());
                    self.high_priority_event = Some(Event::AppWillResignActive);
                    // For some reason, if we don't pause event polling, we will
                    // never finish handling the event.
                    // TODO: Add a mechanism for re-enabling polling, if at some
                    // point we support returning touchHLE to the foreground.
                    self.enable_event_polling = false;
                    continue;
                }
                E::AppTerminating { .. } => {
                    log!("Received app-will-terminate event.");
                    assert!(self.high_priority_event.is_none());
                    self.high_priority_event = Some(Event::AppWillTerminate);
                    self.enable_event_polling = false;
                    continue;
                }
                E::FingerUp {
                    timestamp,
                    finger_id,
                    x,
                    y,
                    ..
                }
                | E::FingerMotion {
                    timestamp,
                    finger_id,
                    x,
                    y,
                    ..
                }
                | E::FingerDown {
                    timestamp,
                    finger_id,
                    x,
                    y,
                    ..
                } => {
                    let kind = match event {
                        E::FingerDown { .. } => TouchKind::Down,
                        E::FingerMotion { .. } => TouchKind::Move,
                        _ => TouchKind::Up,
                    };
                    let abs_coords = finger_absolute_coords(self, (x, y));
                    if Self::handle_home_button_touch(
                        self.home_button_geometry(),
                        self.viewport(),
                        &mut self.home_button_press,
                        kind,
                        finger_id,
                        abs_coords,
                    ) {
                        continue;
                    }
                    log_dbg!("Starting multi-touch for {:?}", event);
                    // To implement multi-touch we accumulate here same touch
                    // events at the same timestamp. This is consistent with
                    // UIKit, but could be broken if events come out of order.
                    // (in worst case we separate multi-touches in several ones)
                    // TODO: handle out of order touches
                    let curr_timestamp = timestamp;
                    let coords = transform_input_coords(self, abs_coords, false);
                    log_dbg!("Finger event x {}, y {}, coords {:?}", x, y, coords);
                    let mut map = HashMap::from([(FingerId::Touch(finger_id), coords)]);
                    while let Some(next) = self.event_pump.poll_event() {
                        match next {
                            E::Unknown { .. } => (),
                            _ => log_dbg!("Next possible multi-touch event: {:?}", next),
                        }
                        match next {
                            E::FingerUp {
                                timestamp,
                                finger_id,
                                x,
                                y,
                                ..
                            }
                            | E::FingerMotion {
                                timestamp,
                                finger_id,
                                x,
                                y,
                                ..
                            }
                            | E::FingerDown {
                                timestamp,
                                finger_id,
                                x,
                                y,
                                ..
                            } if timestamp == curr_timestamp && next.is_same_kind_as(&event) => {
                                let abs_coords = finger_absolute_coords(self, (x, y));
                                if Self::handle_home_button_touch(
                                    self.home_button_geometry(),
                                    self.viewport(),
                                    &mut self.home_button_press,
                                    kind,
                                    finger_id,
                                    abs_coords,
                                ) {
                                    continue;
                                }
                                let coords = transform_input_coords(self, abs_coords, false);
                                map.insert(FingerId::Touch(finger_id), coords);
                            }
                            E::MultiGesture { timestamp, .. } if timestamp == curr_timestamp => {
                                // TODO: handle gestures
                                continue;
                            }
                            _ => {
                                // event_pump doesn't have a method to peek on
                                // events, so we keep track of an unconsumed
                                // one from a previous loop iteration
                                assert!(previous_event.is_none());
                                previous_event = Some(next);
                                break;
                            }
                        }
                    }
                    log_dbg!("Finishing multi-touch for {:?} with {:?}", event, map);
                    match event {
                        E::FingerUp { .. } => Event::TouchesUp(map),
                        E::FingerMotion { .. } => Event::TouchesMove(map),
                        E::FingerDown { .. } => Event::TouchesDown(map),
                        _ => unreachable!(),
                    }
                }
                E::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::F12),
                    ..
                } => {
                    // Log this so you can tell when touchHLE has received
                    // the event but it's stuck in the queue.
                    echo!("F12 pressed, EnterDebugger event queued.");
                    Event::EnterDebugger
                }
                E::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => {
                    echo!("Esc pressed, HomeButton event queued.");
                    Event::HomeButton
                }
                E::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::AcBack),
                    ..
                } => {
                    echo!("Back button pressed, HomeButton event queued.");
                    Event::HomeButton
                }
                E::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Backspace),
                    ..
                } => {
                    log_dbg!("SDL TextInput Backspace");
                    Event::TextInput(TextInputEvent::Backspace)
                }
                E::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Return),
                    ..
                } => {
                    log_dbg!("SDL TextInput Return");
                    Event::TextInput(TextInputEvent::Return)
                }
                E::TextInput { text, .. } => {
                    log_dbg!("SDL TextInput {}", text);
                    Event::TextInput(TextInputEvent::Text(text))
                }
                _ => continue,
            })
        }

        if let Some(press) = &mut self.home_button_press {
            if !press.triggered {
                press.triggered = true;
                echo!("On-screen Home button touched, HomeButton event queued.");
                self.event_queue.push_back(Event::HomeButton);
            }
        }

        if let Some(since) = self.controller_back_held_since {
            if since.elapsed() >= CONTROLLER_BACK_HOLD {
                // Only once per hold.
                self.controller_back_held_since = None;
                echo!("Controller Back button held, HomeButton event queued.");
                self.event_queue.push_back(Event::HomeButton);
            }
        }

        if controller_updated {
            let (new_x, new_y, pressed, pressed_changed, moved) =
                self.update_virtual_cursor(options);
            self.event_queue
                .push_back(match (pressed, pressed_changed, moved) {
                    (true, true, _) => {
                        let coords = transform_input_coords(self, (new_x, new_y), false);
                        Event::TouchesDown(HashMap::from([(FingerId::VirtualCursor, coords)]))
                    }
                    (false, true, _) => {
                        let coords = transform_input_coords(self, (new_x, new_y), false);
                        Event::TouchesUp(HashMap::from([(FingerId::VirtualCursor, coords)]))
                    }
                    (true, _, true) => {
                        let coords = transform_input_coords(self, (new_x, new_y), false);
                        Event::TouchesMove(HashMap::from([(FingerId::VirtualCursor, coords)]))
                    }
                    _ => return,
                });
        }
    }

    /// Show the on-screen Home button, on platforms without another way to
    /// return to the app picker (iOS). It's for apps, not the app picker.
    pub fn enable_on_screen_home_button(&mut self) {
        self.on_screen_home_button = ON_SCREEN_HOME_BUTTON_PLATFORM;
    }

    /// Where the on-screen Home button is, if it's shown: its centre and
    /// radius, in drawable pixels from the top left.
    ///
    /// It sits in the black bars beside the app: below it (where the original
    /// iPhone's Home button was) or else to its right. So it never covers the
    /// app or takes its touches. Its centre is a little nearer the app than
    /// the screen's edge, clear of the home indicator and Dynamic Island. If
    /// the bars are too narrow, there is no button.
    fn home_button_geometry(&self) -> Option<(f32, f32, f32)> {
        if !self.on_screen_home_button {
            return None;
        }
        let (vx, vy, vw, vh) = self.viewport();
        let (screen_width, screen_height) = self.window.drawable_size();
        let pixels_per_point = screen_width as f32 / self.window.size().0 as f32;
        let bottom_bar = (screen_height - (vy + vh)) as f32;
        let right_bar = (screen_width - (vx + vw)) as f32;
        let (bar, x, y) = if bottom_bar > 0.0 && bottom_bar >= right_bar {
            (
                bottom_bar,
                screen_width as f32 / 2.0,
                (vy + vh) as f32 + bottom_bar * 0.45,
            )
        } else if right_bar > 0.0 {
            (
                right_bar,
                (vx + vw) as f32 + right_bar * 0.45,
                screen_height as f32 / 2.0,
            )
        } else {
            return None;
        };
        // 44 points across, Apple's minimum size for something to tap, if
        // there's room.
        let radius = (HOME_BUTTON_RADIUS * pixels_per_point).min(bar * HOME_BUTTON_BAR_RATIO);
        (radius >= HOME_BUTTON_MIN_RADIUS * pixels_per_point).then_some((x, y, radius))
    }

    /// The on-screen Home button to draw over the app, if it's shown.
    pub fn home_button_overlay(&self) -> Option<HomeButtonOverlay> {
        let (x, y, radius) = self.home_button_geometry()?;
        let (screen_width, screen_height) = self.window.drawable_size();
        let pressed = self.home_button_press.is_some();
        Some(HomeButtonOverlay {
            x,
            y,
            radius,
            pressed,
            screen_width,
            screen_height,
        })
    }

    /// Handle a touch that might be on the on-screen Home button, which is at
    /// `geometry` (see [Self::home_button_geometry]). Returns [true] if it is,
    /// in which case it must not be passed on to the app. (This takes the
    /// press state rather than `&mut self`, so it can be used while events are
    /// being queued.)
    fn handle_home_button_touch(
        geometry: Option<(f32, f32, f32)>,
        (vx, vy, vw, vh): (u32, u32, u32, u32),
        home_button_press: &mut Option<HomeButtonPress>,
        kind: TouchKind,
        finger_id: i64,
        (x, y): (f32, f32),
    ) -> bool {
        let Some((button_x, button_y, radius)) = geometry else {
            return false;
        };
        let distance = (x - button_x).hypot(y - button_y);
        // Apps never get touches outside their area, so the button can take
        // any that start nearby, well beyond what's drawn: a fingertip's
        // touch point often lands below where the finger is aimed.
        let outside_app =
            x < vx as f32 || y < vy as f32 || x >= (vx + vw) as f32 || y >= (vy + vh) as f32;
        if matches!(kind, TouchKind::Down) && outside_app {
            log_dbg!(
                "Touch outside the app at ({}, {}) pixels; on-screen Home button at ({}, {}), radius {}",
                x,
                y,
                button_x,
                button_y,
                radius
            );
        }
        match kind {
            TouchKind::Down
                if home_button_press.is_none() && outside_app && distance <= radius * 2.5 =>
            {
                *home_button_press = Some(HomeButtonPress {
                    finger_id,
                    triggered: false,
                });
                true
            }
            TouchKind::Move => home_button_press
                .as_ref()
                .is_some_and(|press| press.finger_id == finger_id),
            TouchKind::Up => {
                if home_button_press
                    .as_ref()
                    .is_some_and(|press| press.finger_id == finger_id)
                {
                    *home_button_press = None;
                    true
                } else {
                    false
                }
            }
            TouchKind::Down => false,
        }
    }

    /// Pop an event from the queue (in FIFO order, except for high priority
    /// events)
    pub fn pop_event(&mut self) -> Option<Event> {
        self.high_priority_event
            .take()
            .or_else(|| self.event_queue.pop_front())
    }

    fn controller_added(&mut self, joystick_idx: u32) {
        let Ok(controller) = self.controller_ctx.open(joystick_idx) else {
            log!("Warning: A new controller was connected, but it couldn't be accessed!");
            return;
        };

        let controller_name = controller.name();
        if env::consts::OS == "android" && controller_name.starts_with("uinput-") {
            log!("ignoring fingerprint device: {}", controller_name);
            return;
        }
        log!(
            "New controller connected: {}. Left stick = device tilt. Right stick = touch input (press the stick or shoulder button to tap/hold).",
            controller_name
        );
        self.controllers.push(controller);
    }
    fn controller_removed(&mut self, instance_id: u32) {
        let Some(idx) = self
            .controllers
            .iter()
            .position(|controller| controller.instance_id() == instance_id)
        else {
            return;
        };
        let controller = self.controllers.remove(idx);
        log!("Warning: Controller disconnected: {}", controller.name());
    }
    pub fn print_accelerometer_notice(&self, options: &Options) {
        log!("This app uses the accelerometer.");

        if !self.controllers.is_empty() && options.analog_stick_tilt_controls {
            log!("Your connected controller's left analog stick will be used for accelerometer simulation.");
            if self.accelerometer.is_some() {
                log!("Disconnect the controller if you want to use your device's accelerometer.");
            }
        } else if self.accelerometer.is_some() {
            log!("Your device's accelerometer will be used for accelerometer simulation.");
            if options.analog_stick_tilt_controls {
                log!("Connect a controller if you would prefer to use an analog stick.");
            }
        } else if self.controllers.is_empty() && options.analog_stick_tilt_controls {
            log!("Connect a controller to get accelerometer simulation.");
        }

        if self.accelerometer.is_none() {
            log!(
                "You can {}hold right click and move the cursor to simulate the accelerometer.",
                if options.analog_stick_tilt_controls {
                    "also "
                } else {
                    ""
                }
            );
        }
    }

    /// Get the real or simulated accelerometer output.
    /// See also [crate::frameworks::uikit::ui_accelerometer].
    pub fn get_acceleration(&self, options: &Options) -> (f32, f32, f32) {
        if self.controllers.is_empty() || !options.analog_stick_tilt_controls {
            if let Some(ref accelerometer) = self.accelerometer {
                let data = accelerometer.get_data().unwrap();
                let sdl2::sensor::SensorData::Accel(data) = data else {
                    panic!();
                };
                let [x, y, z] = data;
                // UIAcceleration reports acceleration towards gravity, but SDL2
                // reports acceleration away from gravity.
                let (x, y, z) = (-x, -y, -z);
                // UIAcceleration reports acceleration in units of g-force, but
                // SDL2 reports acceleration in units of m/s^2.
                let gravity: f32 = 9.80665; // SDL_STANDARD_GRAVITY
                let (x, y, z) = (x / gravity, y / gravity, z / gravity);
                return (x, y, z);
            }
        }

        let (x, y) = if self
            .virtual_accelerometer_last
            .is_some_and(|(_x, _y, right_click_hold)| right_click_hold)
        {
            self.virtual_accelerometer_last
                .map(|(x, y, _right_click_hold)| (x, y))
                .unwrap()
        } else {
            // Get left analog stick input. The range is [-1, 1] on each axis.
            let (x, y, _) = self.get_controller_stick(options, true);
            (x, y)
        };

        // Correct for window rotation
        let [x, y] = self.rotation_matrix().inverse().unwrap().transform([x, y]);
        let (x, y) = (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)); // just in case

        // Let's simulate tilting the device based on the analog stick inputs.
        //
        // If an iPhone is lying flat on its back, level with the ground, and it
        // is on Earth, the accelerometer will report approximately (0, 0, -1).
        // The acceleration x and y axes are aligned with the screen's x and y
        // axes. +x points to the right of the screen, +y points to the top of
        // the screen, and +z points away from the screen. In the example
        // scenario, the z axis is parallel to gravity.

        let gravity: [f32; 3] = [0.0, 0.0, -1.0];

        let neutral_x = options.x_tilt_offset.to_radians();
        let neutral_y = options.y_tilt_offset.to_radians();
        let x_rotation_range = options.x_tilt_range.to_radians() / 2.0;
        let y_rotation_range = options.y_tilt_range.to_radians() / 2.0;
        // (x, y) are swapped because the controller Y axis usually corresponds
        // to forward/backward movement, but rotating about the Y axis means
        // tilting the device left/right.
        let x_rotation = neutral_x - x_rotation_range * y;
        let y_rotation = neutral_y - y_rotation_range * x;
        let matrix =
            Matrix::<3>::y_rotation(y_rotation).multiply(&Matrix::<3>::x_rotation(x_rotation));
        let [x, y, z] = matrix.transform(gravity);

        (x, y, z)
    }

    /// For use when redrawing the screen: Get the cached position, relative to
    /// the viewport, and press state of the analog stick-controlled virtual
    /// cursor, if it is visible.
    pub fn virtual_cursor_visible_at(&self) -> Option<(f32, f32, bool)> {
        let (x, y, pressed, visible) = self.virtual_cursor_last?;
        let (vx, vy, _, _) = self.viewport();
        let (vx, vy) = (vx as f32, vy as f32);
        if visible {
            // When stickyness is in use, the visual cursor movement appears
            // uncomfortably choppy. Showing the un-sticky position is a bit
            // misleading but it *feels* better, and it is documented.
            if let Some((x_unsticky, y_unsticky, _time)) = self.virtual_cursor_last_unsticky {
                Some((x_unsticky - vx, y_unsticky - vy, pressed))
            } else {
                Some((x - vx, y - vy, pressed))
            }
        } else {
            None
        }
    }

    /// Update the virtual cursor's position, click state and visibility, then
    /// return the new position, pressed state, whether the press state changed
    /// and whether the cursor moved.
    fn update_virtual_cursor(&mut self, options: &Options) -> (f32, f32, bool, bool, bool) {
        // Get right analog stick input. The range is [-1, 1] on each axis.
        let (x, y, pressed) = self.get_controller_stick(options, false);

        // The cursor is intended to only show up once you move the analog stick
        // out of its deadzone, or while the button is held.
        let visible = pressed || x != 0.0 || y != 0.0;

        // Though the analog stick output fits within a square, its actual range
        // is usually a circle enclosed by the square. So we need to cut out the
        // rectangular shape of the screen from that circle within the square.
        let (vx, vy, vw, vh) = self.viewport();
        let (vx, vy, vw, vh) = (vx as f32, vy as f32, vw as f32, vh as f32);

        let (x, y) = {
            // Use Pythagoras's theorem to find the largest size the rectangle
            // can have within the circle.
            let ratio = vw / vh;
            let rect_height = (ratio * ratio + 1.0).powf(-0.5);
            let rect_width = ratio * rect_height;

            let x_abs = x.abs().min(rect_width) / rect_width;
            let y_abs = y.abs().min(rect_height) / rect_height;
            (x_abs.copysign(x), y_abs.copysign(y))
        };

        // Convert to on-screen window co-ordinates
        let x = (x / 2.0 + 0.5) * vw + vx;
        let y = (y / 2.0 + 0.5) * vh + vy;

        let (old_x, old_y, old_pressed, _old_visible) =
            self.virtual_cursor_last.unwrap_or_default();

        let (x, y) = if let Some((smoothing_strength, sticky_radius)) =
            options.stabilize_virtual_cursor
        {
            let new_time = Instant::now();

            let (old_x_unsticky, old_y_unsticky, old_time) = self
                .virtual_cursor_last_unsticky
                .unwrap_or((0.0, 0.0, new_time));

            let delta_t = new_time.saturating_duration_since(old_time).as_secs_f32();

            // Apply a feedback-based smoothing with exponential decay, to try
            // to dampen shakiness in the stick movement.

            let smooth = |old: f32, new: f32| -> f32 {
                if smoothing_strength != 0.0 {
                    let lerp_factor = 1.0 - (0.5_f32).powf(delta_t * (1.0 / smoothing_strength));
                    old + (new - old) * lerp_factor
                } else {
                    new
                }
            };

            let new_x_unsticky = smooth(old_x_unsticky, x);
            let new_y_unsticky = smooth(old_y_unsticky, y);

            self.virtual_cursor_last_unsticky = Some((new_x_unsticky, new_y_unsticky, new_time));

            // Make the reported position "sticky" within a certain radius, i.e.
            // if the new position's distance from the old one is within the
            // radius, report no change in position.

            if (new_x_unsticky - old_x).hypot(new_y_unsticky - old_y) < sticky_radius {
                (old_x, old_y)
            } else {
                (new_x_unsticky, new_y_unsticky)
            }
        } else {
            (x, y)
        };

        self.virtual_cursor_last = Some((x, y, pressed, visible));

        (
            x,
            y,
            pressed,
            pressed != old_pressed,
            x != old_x || y != old_y,
        )
    }

    /// Get the summed X and Y positions and button state of the left or right
    /// analog stick of the game controllers. Each axis value is in the range
    /// [-1, 1].
    fn get_controller_stick(&self, options: &Options, left: bool) -> (f32, f32, bool) {
        fn convert_axis(axis: i16, deadzone: f32) -> f32 {
            assert!(deadzone >= 0.0);
            let axis = ((axis as f32) / (i16::MAX as f32)).clamp(-1.0, 1.0);
            let abs_axis = (axis.abs().max(deadzone) - deadzone) / (1.0 - deadzone);
            abs_axis.copysign(axis)
        }

        let (mut x, mut y) = (0.0, 0.0);
        let mut pressed = false;
        for controller in &self.controllers {
            use sdl2::controller::{Axis, Button};
            let (x_axis, y_axis, button1, button2) = if left {
                (
                    Axis::LeftX,
                    Axis::LeftY,
                    Button::LeftStick,
                    Button::LeftShoulder,
                )
            } else {
                (
                    Axis::RightX,
                    Axis::RightY,
                    Button::RightStick,
                    Button::RightShoulder,
                )
            };
            x += convert_axis(controller.axis(x_axis), options.deadzone);
            y += convert_axis(controller.axis(y_axis), options.deadzone);
            pressed |= controller.button(button1);
            pressed |= controller.button(button2);
        }
        let (x, y) = (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0));

        (x, y, pressed)
    }

    /// Create an SDL audio subsystem from the same SDL context as the window.
    /// This shares SDL's lifetime refcount, so the audio device and the window
    /// keep SDL alive independently of each other.
    pub fn create_audio_subsystem(&self) -> Result<sdl2::AudioSubsystem, String> {
        self._sdl_ctx.audio()
    }

    pub fn create_gl_context(&self, version: GLVersion) -> Result<GLContext, String> {
        let attr = self.video_ctx.gl_attr();
        match version {
            GLVersion::GLES11 => {
                attr.set_context_version(1, 1);
                attr.set_context_profile(sdl2::video::GLProfile::GLES);
            }
            GLVersion::GL21Compat => {
                attr.set_context_version(2, 1);
                attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
            }
        }

        let gl_ctx = self.window.gl_create_context()?;

        Ok(GLContext(gl_ctx))
    }

    pub fn gl_get_proc_address(&self, procname: &str) -> *const std::ffi::c_void {
        // For some reason, rust-sdl2 uses *const (), but () is not meant to be
        // used for void pointees (just void results), so let's fix that.
        self.video_ctx.gl_get_proc_address(procname) as *const _
    }

    pub fn set_share_with_current_context(&self, value: bool) {
        self.video_ctx
            .gl_attr()
            .set_share_with_current_context(value)
    }

    pub unsafe fn make_gl_context_current(&self, gl_ctx: &GLContext) {
        self.window.gl_make_current(&gl_ctx.0).unwrap();
    }

    /// Make the internal OpenGL ES context (for splash screen and UI rendering)
    /// current.
    #[must_use]
    pub fn make_internal_gl_ctx_current<'win>(&'win mut self) -> Box<dyn GLES + 'win> {
        // The invariant is held up here - since the instance we return is
        // bound to the lifetime of window, it can't outlive the internal GL
        // context and can't outlive the window.
        let gl_ins = unsafe {
            self.internal_gl_ins
                .as_mut()
                .unwrap()
                .make_current_unchecked_for_window(
                    &mut |gl_ctx| self.window.gl_make_current(&gl_ctx.0).unwrap(),
                    &mut |s| self.video_ctx.gl_get_proc_address(s) as *const _,
                )
        };
        gl_ins
    }

    fn display_splash(&mut self) {
        assert!(self.splash_image.is_some());

        self.rebind_framebuffer();
        self.swap_window();

        // OpenGL ES expects bottom-to-top row order for image data, but our
        // image data will be top-to-bottom. A reflection transform compensates.
        let matrix = self.rotation_matrix().multiply(&Matrix::y_flip());
        let (vx, vy, vw, vh) = self.gl_viewport();
        let viewport = (vx, vy + self.viewport_y_offset(), vw, vh);

        let image = self.splash_image.as_ref().unwrap();

        unsafe {
            let mut gl_ctx = self
                .internal_gl_ins
                .as_mut()
                .unwrap()
                .make_current_unchecked_for_window(
                    &mut |gl_ctx| self.window.gl_make_current(&gl_ctx.0).unwrap(),
                    &mut |s| self.video_ctx.gl_get_proc_address(s) as *const _,
                );

            use crate::gles::gles11_raw as gles11; // constants only

            let mut texture = 0;
            gl_ctx.GenTextures(1, &mut texture);
            gl_ctx.BindTexture(gles11::TEXTURE_2D, texture);
            let (width, height) = image.dimensions();
            gl_ctx.TexImage2D(
                gles11::TEXTURE_2D,
                0,
                gles11::RGBA as _,
                width as _,
                height as _,
                0,
                gles11::RGBA,
                gles11::UNSIGNED_BYTE,
                image.pixels().as_ptr() as *const _,
            );
            gl_ctx.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_MIN_FILTER,
                gles11::LINEAR as _,
            );
            gl_ctx.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_MAG_FILTER,
                gles11::LINEAR as _,
            );

            // iOS and PowerVR only support NPOT texture with CLAMP_TO_EDGE
            // It doesn't matter for texture data from an image like the
            // splash screen, we don't want to deal with striping from
            // NPOT to POT conversion.
            gl_ctx.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_WRAP_S,
                gles11::CLAMP_TO_EDGE as _,
            );

            gl_ctx.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_WRAP_T,
                gles11::CLAMP_TO_EDGE as _,
            );

            present_frame(
                gl_ctx.as_mut(),
                viewport,
                matrix,
                /* virtual_cursor_visible_at: */ None,
                /* home_button: */ None,
                None,
            );

            gl_ctx.DeleteTextures(1, &texture);
        };

        self.swap_window();

        // hold onto GL context so the image doesn't disappear, and hold
        // onto image so we can rotate later if necessary
    }

    /// Swap front-buffer and back-buffer so the result of OpenGL rendering is
    /// presented.
    pub fn swap_window(&mut self) {
        // Deliberately does NOT rebind to the internal context's renderbuffer
        // here: on iOS that would call make_internal_gl_ctx_current() and make
        // the internal GL context current right before the swap, causing SDL to
        // present the internal (empty) view instead of whichever context's view
        // was actually just rendered. The fast EAGL present path
        // (present_renderbuffer) draws into the guest context's own view and
        // relies on that same context still being current when we swap. SDL's
        // iOS swapBuffers already re-asserts the current view's context and
        // rebinds its renderbuffer internally, so no rebind is needed here.
        // That rebind changes the current context's renderbuffer binding,
        // though, so the fast path restores the app's afterwards.
        self.window.gl_swap_window();
    }

    #[cfg(target_os = "ios")]
    fn get_sys_wm_info_ios(&self) -> SDL_SysWMinfoUIKit {
        use sdl2_sys::{SDL_GetVersion, SDL_GetWindowWMInfo, SDL_SysWMinfo};
        use std::mem::MaybeUninit;

        unsafe {
            let mut wminfo = MaybeUninit::<SDL_SysWMinfo>::zeroed();
            SDL_GetVersion(&raw mut (*wminfo.as_mut_ptr()).version);
            SDL_GetWindowWMInfo(self.window.raw(), wminfo.as_mut_ptr());

            let uikit_info_ptr = (&raw const (*wminfo.as_ptr()).info) as *const SDL_SysWMinfoUIKit;
            uikit_info_ptr.read()
        }
    }

    /// The framebuffer object that backs the internal (compositor) context's
    /// own on-screen view, for use when presenting the composited frame through
    /// that context (see `core_animation::composition`).
    ///
    /// On iOS every GL context owns a separate view with its own
    /// FBO/renderbuffer, and framebuffer objects are never shared between
    /// contexts, so the composite must be drawn into *this* framebuffer for
    /// SDL's `swapBuffers` (which presents the current context's view
    /// renderbuffer) to show it. SDL reports the framebuffer of whichever view
    /// is currently attached to the window, and making the internal context
    /// current is what attaches its view — so this makes the internal context
    /// current first, then reads the value. On desktop GL there is no such
    /// per-view framebuffer and 0 is the genuine default framebuffer.
    pub fn internal_ctx_default_framebuffer(&mut self) -> u32 {
        #[cfg(target_os = "ios")]
        {
            // Attach the internal context's view (drops immediately; the SDL
            // context stays current afterwards).
            let _ = self.make_internal_gl_ctx_current();
            self.get_sys_wm_info_ios().framebuffer
        }
        #[cfg(not(target_os = "ios"))]
        0
    }

    pub fn rebind_framebuffer(&mut self) {
        #[cfg(target_os = "ios")]
        {
            use crate::gles::gles11_raw as gles11;
            let framebuffer = self.get_sys_wm_info_ios().framebuffer;
            unsafe {
                self.make_internal_gl_ctx_current()
                    .BindFramebufferOES(gles11::FRAMEBUFFER_OES, framebuffer);
            }
        }
    }

    /// Consider the emulated device to be rotated to a particular orientation.
    ///
    /// On a PC or laptop, this will make the window be rotated so the app
    /// content appears upright. On a mobile device, this might do something
    /// else, because the user can physically rotate the screen.
    pub fn rotate_device(&mut self, new_orientation: DeviceOrientation) {
        assert!(self.on_main_stack);
        if new_orientation == self.device_orientation {
            return;
        }

        if !self.fullscreen && !Self::rotatable_fullscreen() {
            let (width, height) = if Self::rotatable_fullscreen() {
                set_sdl2_orientation(new_orientation);
                rotate_fullscreen_size(new_orientation, self.window.size())
            } else {
                size_for_orientation(self.portrait_size(), new_orientation, self.scale_hack)
            };

            // macOS quirk: when resizing the window, the new framebuffer's size
            // is apparently max(new_size, old_size) in each dimension, but the
            // viewport is positioned wrong on the y axis for some reason, so we
            // need to apply an offset.
            // Recreating the OpenGL context was an alternative workaround, but
            // that apparently stops other OpenGL contexts drawing to the
            // framebuffer!
            #[cfg(target_os = "macos")]
            {
                let (_old_width, old_height) = self.window.size();
                self.max_height = self.max_height.max(old_height).max(height);
                self.viewport_y_offset = self.max_height - height;
            }

            self.window.set_size(width, height).unwrap();
        }

        if Self::rotatable_fullscreen() {
            set_sdl2_orientation(new_orientation);
            // Hack: from reading SDL2's source code, it seems that SDL2 will
            // only re-do the orientation when changing whether a window is
            // "resizeable" (can be rotated). You can't set the resizeable state
            // on a fullscreen window, so it must be temporarily stop being
            // fulscreen.
            // Apparently, doing this does result in resizing the window.
            self.window
                .set_fullscreen(sdl2::video::FullscreenType::Off)
                .unwrap();
            unsafe {
                let window_raw = self.window.raw();
                sdl2_sys::SDL_SetWindowResizable(window_raw, sdl2_sys::SDL_bool::SDL_FALSE);
                sdl2_sys::SDL_SetWindowResizable(window_raw, sdl2_sys::SDL_bool::SDL_TRUE);
            }
            // iOS rotates the screen asynchronously, and SDL won't go
            // fullscreen in a mode that doesn't match the screen yet, so let
            // the rotation finish (pumping keeps the events queued).
            let mut tries = 0;
            while let Err(e) = self
                .window
                .set_fullscreen(sdl2::video::FullscreenType::True)
            {
                tries += 1;
                if tries == 100 {
                    log!("Warning: couldn't make the window fullscreen: {}", e);
                    break;
                }
                self.event_pump.pump_events();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        self.device_orientation = new_orientation;

        if self.splash_image.is_some() {
            self.display_splash();
        }
    }

    pub fn device_family(&self) -> DeviceFamily {
        self.device_family
    }

    /// Returns the current device orientation
    pub fn current_rotation(&self) -> DeviceOrientation {
        self.device_orientation
    }

    /// Get the size in pixels of the window without rotation or scaling.
    ///
    /// The aspect ratio, scale and orientation reflect the guest app's view of
    /// the world.
    pub fn size_unrotated_unscaled(&self) -> (u32, u32) {
        size_for_orientation(
            self.portrait_size(),
            DeviceOrientation::Portrait,
            NonZeroU32::new(1).unwrap(),
        )
    }

    /// Get the region of the on-screen window (x, y, width, height) used to
    /// display the app content.
    ///
    /// The aspect ratio of this region always reflects the guest app's view of
    /// the world, but the scale and orientation might not.
    pub fn viewport(&self) -> (u32, u32, u32, u32) {
        let (app_width, app_height) = size_for_orientation(
            self.portrait_size(),
            self.device_orientation,
            self.scale_hack,
        );
        if !self.fullscreen && !Self::rotatable_fullscreen() {
            return (0, 0, app_width, app_height);
        }

        let (screen_width, screen_height) = self.window.drawable_size();
        let pixels_per_point = screen_width as f32 / self.window.size().0 as f32;

        let app_aspect = app_width as f32 / app_height as f32;
        let fit = |width: u32, height: u32| {
            if app_aspect < width as f32 / height as f32 {
                ((height as f32 * app_aspect).round() as u32, height)
            } else {
                (width, (width as f32 / app_aspect).round() as u32)
            }
        };
        let (scaled_width, scaled_height) = fit(screen_width, screen_height);

        // If the bars beside the app are too narrow for the on-screen Home
        // button (e.g. an iPad app on a 4:3 iPad), shrink the app a little to
        // make room for it along the screen's longer side.
        if self.on_screen_home_button {
            let min_bar = HOME_BUTTON_MIN_RADIUS / HOME_BUTTON_BAR_RATIO * pixels_per_point;
            let bottom_bar = (screen_height - scaled_height) as f32 / 2.0;
            let right_bar = (screen_width - scaled_width) as f32 / 2.0;
            if bottom_bar < min_bar && right_bar < min_bar {
                let room = (HOME_BUTTON_RADIUS / HOME_BUTTON_BAR_RATIO * pixels_per_point).ceil();
                let room = room as u32;
                let (x, y, width, height) = if screen_height >= screen_width {
                    let (width, height) = fit(screen_width, screen_height - room);
                    let y = (screen_height - room - height) / 2;
                    ((screen_width - width) / 2, y, width, height)
                } else {
                    let (width, height) = fit(screen_width - room, screen_height);
                    let x = (screen_width - room - width) / 2;
                    (x, (screen_height - height) / 2, width, height)
                };
                return (x, y, width, height);
            }
        }

        let x = (screen_width - scaled_width) / 2;
        let y = (screen_height - scaled_height) / 2;
        (x, y, scaled_width, scaled_height)
    }

    /// [Self::viewport] for `glViewport()`, which counts y from the bottom.
    pub fn gl_viewport(&self) -> (u32, u32, u32, u32) {
        let (x, y, width, height) = self.viewport();
        if !self.fullscreen && !Self::rotatable_fullscreen() {
            return (x, y, width, height);
        }
        let (_, drawable_height) = self.window.drawable_size();
        (x, drawable_height - y - height, width, height)
    }

    /// The emulated screen's size in portrait, in points.
    pub fn portrait_size(&self) -> (u32, u32) {
        self.screen_size
            .unwrap_or_else(|| self.device_family.portrait_size())
    }

    /// Draw UIKit at the screen's real resolution rather than one pixel per
    /// point, so text is sharp. This is for the app picker: apps would see
    /// bigger bitmaps than they expect.
    pub fn enable_hidpi_ui(&mut self) {
        self.hidpi_ui = true;
    }

    /// Pixels per point UIKit draws at (see [Self::enable_hidpi_ui]), which is
    /// 1 unless that's enabled.
    pub fn ui_scale(&self) -> f32 {
        if !self.hidpi_ui {
            return 1.0;
        }
        let (_, _, viewport_width, _) = self.viewport();
        let (width, _) = size_for_orientation(
            self.portrait_size(),
            self.device_orientation,
            NonZeroU32::new(1).unwrap(),
        );
        (viewport_width as f32 / width as f32).max(1.0)
    }

    /// Whether [Self::fit_screen_to_device] changed the screen's size.
    pub fn is_screen_fitted(&self) -> bool {
        self.screen_size.is_some()
    }

    /// The emulated screen's safe area insets (top, left, bottom, right) in
    /// its own points. These are only non-zero after
    /// [Self::fit_screen_to_device].
    pub fn screen_safe_insets(&self) -> (f32, f32, f32, f32) {
        self.screen_safe_insets
    }

    /// The emulated screen's points per device point.
    pub fn screen_scale(&self) -> f32 {
        self.screen_scale
    }

    /// Make the emulated screen the same shape as the device's, keeping its
    /// width, so the app picker fills modern iPhones. Apps keep the screen of
    /// the device they were made for.
    pub fn fit_screen_to_device(&mut self) {
        if !Self::rotatable_fullscreen() {
            return;
        }
        // The app picker only has a portrait layout.
        let (width, height) = self.window.size();
        if height <= width {
            return;
        }
        let device = (width as f32, height as f32);
        if let Some(size) = screen_size_for_device(self.device_family, device) {
            let scale = size.0 as f32 / device.0;
            let (top, left, bottom, right) = safe_area_insets();
            self.screen_size = Some(size);
            self.screen_scale = scale;
            self.screen_safe_insets = (top * scale, left * scale, bottom * scale, right * scale);
        }
    }

    /// Special offset to add to y co-ordinates, only when drawing to screen.
    pub fn viewport_y_offset(&self) -> u32 {
        #[cfg(target_os = "macos")]
        return self.viewport_y_offset;
        #[cfg(not(target_os = "macos"))]
        return 0;
    }

    /// Transformation matrix for transforming between the window's co-ordinate
    /// space and the app's original co-ordinate space when rotation is in use
    /// (see [Self::rotate_device]). This returns a matrix appropriate for
    /// rotating texture co-ordinates to display the image in the window; when
    /// rotating input co-ordinates, invert the matrix.
    pub fn rotation_matrix(&self) -> Matrix<2> {
        match self.device_orientation {
            DeviceOrientation::Portrait => Matrix::identity(),
            DeviceOrientation::PortraitUpsideDown => Matrix::z_rotation(PI),
            DeviceOrientation::LandscapeLeft => Matrix::z_rotation(-FRAC_PI_2),
            DeviceOrientation::LandscapeRight => Matrix::z_rotation(FRAC_PI_2),
        }
    }

    pub fn is_screen_saver_enabled(&self) -> bool {
        self.video_ctx.is_screen_saver_enabled()
    }
    pub fn set_screen_saver_enabled(&mut self, enabled: bool) {
        assert!(self.on_main_stack);
        match enabled {
            true => self.video_ctx.enable_screen_saver(),
            false => self.video_ctx.disable_screen_saver(),
        }
    }

    pub fn start_text_input(&self) {
        assert!(self.on_main_stack);
        unsafe {
            sdl2_sys::SDL_StartTextInput();
        }
    }
    pub fn stop_text_input(&self) {
        assert!(self.on_main_stack);
        unsafe {
            sdl2_sys::SDL_StopTextInput();
        }
    }

    pub fn on_main_stack(&self) -> bool {
        self.on_main_stack
    }
}

/// The emulated screen for a portrait device of `device` size in points: as
/// wide as the device family's (a third wider on an iPad, so things are 3/4
/// the size and not huge), and the same shape as the device. [None] if that's
/// no bigger than usual.
fn screen_size_for_device(family: DeviceFamily, device: (f32, f32)) -> Option<(u32, u32)> {
    let (mut width, usual_height) = family.portrait_size();
    let (device_width, device_height) = device;
    if device_width >= width as f32 * 2.0 {
        width = (width as f32 * 4.0 / 3.0).round() as u32;
    }
    let height = (width as f32 * device_height / device_width).round() as u32;
    (height > usual_height).then_some((width, height))
}

/// The key window's safe area insets in points (top, left, bottom, right).
fn safe_area_insets() -> (f32, f32, f32, f32) {
    #[cfg(target_os = "ios")]
    unsafe {
        use std::os::raw::{c_char, c_void};

        #[link(name = "objc")]
        extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn objc_msgSend();
        }

        #[repr(C)]
        struct UIEdgeInsets {
            top: f64,
            left: f64,
            bottom: f64,
            right: f64,
        }

        type Id = *mut c_void;
        let send: unsafe extern "C" fn(Id, Id) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let send_insets: unsafe extern "C" fn(Id, Id) -> UIEdgeInsets =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let sel = |name: &[u8]| sel_registerName(name.as_ptr() as *const _);
        let app = send(
            objc_getClass(b"UIApplication\0".as_ptr() as *const _),
            sel(b"sharedApplication\0"),
        );
        let windows = send(app, sel(b"windows\0"));
        let window = send(windows, sel(b"firstObject\0"));
        if window.is_null() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let insets = send_insets(window, sel(b"safeAreaInsets\0"));
        (
            insets.top as f32,
            insets.left as f32,
            insets.bottom as f32,
            insets.right as f32,
        )
    }
    #[cfg(not(target_os = "ios"))]
    (0.0, 0.0, 0.0, 0.0)
}

/// Whether touchHLE is running on an iPad.
pub fn host_is_ipad() -> bool {
    #[cfg(target_os = "ios")]
    unsafe {
        use std::os::raw::{c_char, c_void};

        #[link(name = "objc")]
        extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn objc_msgSend();
        }

        type Id = *mut c_void;
        let send_id: unsafe extern "C" fn(Id, Id) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let send_int: unsafe extern "C" fn(Id, Id) -> isize =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let device = send_id(
            objc_getClass(b"UIDevice\0".as_ptr() as *const _),
            sel_registerName(b"currentDevice\0".as_ptr() as *const _),
        );
        // UIUserInterfaceIdiomPad
        send_int(
            device,
            sel_registerName(b"userInterfaceIdiom\0".as_ptr() as *const _),
        ) == 1
    }
    #[cfg(not(target_os = "ios"))]
    false
}

#[cfg(target_os = "ios")]
fn open_url_host_ios(url: &str) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_void};

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    unsafe {
        let ns_url_class = objc_getClass(b"NSURL\0".as_ptr() as *const _);
        let ns_string_class = objc_getClass(b"NSString\0".as_ptr() as *const _);
        let ui_application_class = objc_getClass(b"UIApplication\0".as_ptr() as *const _);
        let ns_dictionary_class = objc_getClass(b"NSDictionary\0".as_ptr() as *const _);

        if ns_url_class.is_null()
            || ns_string_class.is_null()
            || ui_application_class.is_null()
            || ns_dictionary_class.is_null()
        {
            return Err("Couldn't find required Objective-C classes".to_string());
        }

        let url_cstr = CString::new(url).map_err(|e| e.to_string())?;

        let objc_msgSend_ptr = objc_msgSend as unsafe extern "C" fn();

        let sel_string_with_utf8 =
            sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const _);
        let ns_string = (std::mem::transmute::<
            _,
            unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void,
        >(objc_msgSend_ptr))(
            ns_string_class, sel_string_with_utf8, url_cstr.as_ptr()
        );

        let sel_url_with_string = sel_registerName(b"URLWithString:\0".as_ptr() as *const _);
        let ns_url = (std::mem::transmute::<
            _,
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void,
        >(objc_msgSend_ptr))(ns_url_class, sel_url_with_string, ns_string);

        if ns_url.is_null() {
            return Err(format!("Couldn't create NSURL from {:?}", url));
        }

        let sel_shared_application = sel_registerName(b"sharedApplication\0".as_ptr() as *const _);
        let ui_app = (std::mem::transmute::<
            _,
            unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void,
        >(objc_msgSend_ptr))(ui_application_class, sel_shared_application);

        if ui_app.is_null() {
            return Err("Couldn't get shared UIApplication".to_string());
        }

        let sel_dictionary = sel_registerName(b"dictionary\0".as_ptr() as *const _);
        let empty_dict = (std::mem::transmute::<
            _,
            unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void,
        >(objc_msgSend_ptr))(ns_dictionary_class, sel_dictionary);

        let sel_open_url =
            sel_registerName(b"openURL:options:completionHandler:\0".as_ptr() as *const _);
        (std::mem::transmute::<
            _,
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void, *mut c_void),
        >(objc_msgSend_ptr))(
            ui_app,
            sel_open_url,
            ns_url,
            empty_dict,
            std::ptr::null_mut(),
        );

        Ok(())
    }
}

pub fn open_url(env: &mut Environment, url: &str) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        env.on_parent_stack_in_coroutine(|_, _| open_url_host_ios(url))
    }

    #[cfg(not(target_os = "ios"))]
    env.on_parent_stack_in_coroutine(|_, _| sdl2::url::open_url(url).map_err(|e| e.to_string()))
}

impl Window {
    /// Show a game's alert (`UIAlertView`) and wait for a button to be
    /// tapped. Returns its index, or [None] if the alert was closed without
    /// one (on desktop, the window's close button).
    pub fn show_alert(
        &self,
        title: &str,
        message: &str,
        buttons: &[String],
        cancel_button: Option<usize>,
    ) -> Option<usize> {
        use sdl2::messagebox;
        let buttons: Vec<_> = buttons
            .iter()
            .enumerate()
            .map(|(i, text)| messagebox::ButtonData {
                flags: if Some(i) == cancel_button {
                    messagebox::MessageBoxButtonFlag::ESCAPEKEY_DEFAULT
                } else {
                    messagebox::MessageBoxButtonFlag::NOTHING
                },
                button_id: i as i32,
                text,
            })
            .collect();
        match messagebox::show_message_box(
            messagebox::MessageBoxFlag::INFORMATION,
            &buttons,
            title,
            message,
            &self.window,
            None,
        ) {
            Ok(messagebox::ClickedButton::CustomButton(button)) => Some(button.button_id as usize),
            Ok(messagebox::ClickedButton::CloseButton) => None,
            Err(e) => {
                log!("Warning: couldn't show alert {:?}: {}", title, e);
                None
            }
        }
    }
}

/// Why the last game couldn't start or crashed, for the iOS app picker to
/// show. A messagebox can't be used there before the game has a window: it's
/// never shown, and the app hangs.
pub static START_ERROR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Set when the user picks "Return to App Picker" in the error messagebox.
pub static RETURN_TO_APP_PICKER_AFTER_ERROR: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Show an SDL messagebox for an error (typically after a panic).
///
/// The window argument allows for passing in the parent window for the
/// messagebox, which is not required but should be done if possible.
pub fn show_error_messagebox(window: Option<&Window>, error_message: &str) {
    // On iOS, the app picker shows it instead, with an OK button, so the
    // game always returns there.
    if cfg!(target_os = "ios") {
        let message = format!("iDared 32bit crashed: {error_message}");
        *START_ERROR.lock().unwrap() = Some(message);
        RETURN_TO_APP_PICKER_AFTER_ERROR.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    assert!(window.is_none_or(|win| win.on_main_stack));
    use sdl2::messagebox;
    let mbox = [
        messagebox::ButtonData {
            flags: messagebox::MessageBoxButtonFlag::NOTHING,
            button_id: 0,
            text: "Return to App Picker",
        },
        messagebox::ButtonData {
            flags: messagebox::MessageBoxButtonFlag::NOTHING,
            button_id: 1,
            text: "Close",
        },
    ];

    let Ok(clicked_button) = messagebox::show_message_box(
        messagebox::MessageBoxFlag::ERROR,
        &mbox,
        "iDared 32bit crashed!",
        &format!("iDared 32bit crashed with the following error: {error_message}"),
        window.map(|win| &win.window),
        None,
    ) else {
        panic!("Failed to show message box!");
    };

    match clicked_button {
        messagebox::ClickedButton::CloseButton => {}
        messagebox::ClickedButton::CustomButton(button) => {
            match button.button_id {
                0 => {
                    echo!("Returning to app picker.");
                    RETURN_TO_APP_PICKER_AFTER_ERROR
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                // Close
                1 => {}
                _ => unreachable!(),
            }
        }
    }
}

/// Get current battery state from SDL2.
///
/// Returns:
/// - pct: i32 - percentage of battery remaining.
/// - status: [BatteryState] - the current status of the battery
///   (unplugged, charging, full, etc.)
pub fn get_battery_status() -> (i32, BatteryState) {
    if env::consts::OS == "android" {
        log_once!(
            "Warning: get_battery_status on Android, returning fully charged to avoid SDL crash"
        );
        // Android_JNI_GetPowerInfo is crashing with `JNI DETECTED ERROR IN
        // APPLICATION: JNI ERROR (app bug): jobject is an invalid JNI
        // transition frame reference: 0x7b1bc0c7a0 (use of invalid jobject)`
        // TODO: See if updating SDL fixes that
        return (100, BatteryState::Full);
    }
    let mut pct = 0;
    // Unfortunately, Rust-SDL2 does not expose this function yet.
    // iPhoneOS does not measure the battery in seconds remaining,
    // so we discard this argument.
    let status = unsafe { sdl2_sys::SDL_GetPowerInfo(null_mut(), &mut pct) };
    (
        pct,
        match status {
            SDL_PowerState::SDL_POWERSTATE_UNKNOWN => BatteryState::Unknown,
            SDL_PowerState::SDL_POWERSTATE_ON_BATTERY => BatteryState::OnBattery,
            SDL_PowerState::SDL_POWERSTATE_NO_BATTERY => BatteryState::NoBattery,
            SDL_PowerState::SDL_POWERSTATE_CHARGING => BatteryState::Charging,
            SDL_PowerState::SDL_POWERSTATE_CHARGED => BatteryState::Full,
        },
    )
}

pub fn get_preferred_language_codes(env: &mut Environment) -> Vec<String> {
    env.on_parent_stack_in_coroutine(|_, _| {
        sdl2::locale::get_preferred_locales()
            .map(|loc| loc.lang)
            .collect()
    })
}

pub fn get_preferred_country_codes(env: &mut Environment) -> Vec<String> {
    env.on_parent_stack_in_coroutine(|_, _| {
        sdl2::locale::get_preferred_locales()
            .filter_map(|loc| loc.country)
            .collect()
    })
}
