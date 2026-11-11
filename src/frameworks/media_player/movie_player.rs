/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `MPMoviePlayerController` etc.

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::foundation::{ns_string, ns_url, NSInteger, NSTimeInterval};
use crate::frameworks::uikit::ui_device::{
    UIDeviceOrientation, UIDeviceOrientationLandscapeLeft, UIDeviceOrientationLandscapeRight,
    UIDeviceOrientationPortrait, UIDeviceOrientationUnknown,
};
use crate::objc::{
    id, msg, msg_class, nil, objc_classes, release, retain, todo_objc_setter, ClassExports,
    HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::VecDeque;
use std::ffi::{c_char, CString};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The host's own movie player, if it has one: on iOS, Swift in
/// `iphone/iDared32bit/MoviePlayer.swift`. `play` shows a movie file full
/// screen, with or without controls, in the given orientation (a
/// `UIInterfaceOrientation`, the same values as `UIDeviceOrientation`'s
/// for these, or 0 for the game's own); `finished` says whether it has ended
/// (or was closed by the user, or stopped); `stop` closes it.
#[derive(Clone, Copy)]
struct NativePlayer {
    play: unsafe extern "C" fn(*const c_char, bool, i32),
    finished: unsafe extern "C" fn() -> bool,
    stop: unsafe extern "C" fn(),
}

static NATIVE_PLAYER: OnceLock<NativePlayer> = OnceLock::new();

/// Called by the iOS app's `main()`, like `idared_set_swiftui_app_picker`.
#[no_mangle]
pub extern "C" fn idared_set_movie_player(
    play: unsafe extern "C" fn(*const c_char, bool, i32),
    finished: unsafe extern "C" fn() -> bool,
    stop: unsafe extern "C" fn(),
) {
    let _ = NATIVE_PLAYER.set(NativePlayer {
        play,
        finished,
        stop,
    });
}

#[derive(Default)]
pub struct State {
    active_player: Option<id>,
    /// Various apps (e.g. Crash Bandicoot Nitro Kart 3D and Spore Origins)
    /// create or start a player and await some kind of notification, but can't
    /// handle it if that notification happens immediately. This queue lets us
    /// delay such notifications until the app next returns to the run loop,
    /// which seems to be late enough.
    pending_notifications: VecDeque<(&'static str, id, Instant)>,
    /// The player whose movie the host is playing, retained until it ends.
    native_player: Option<id>,
}
impl State {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env.framework_state.media_player.movie_player
    }
}

type MPMovieScalingMode = NSInteger;
type MPMovieControlStyle = NSInteger;
type MPMovieSourceType = NSInteger;
const MPMovieSourceTypeUnknown: MPMovieSourceType = 0;

type MPMoviePlaybackState = NSInteger;
const MPMoviePlaybackStateStopped: MPMoviePlaybackState = 0;
const MPMoviePlaybackStatePlaying: MPMoviePlaybackState = 1;

const MPMovieControlStyleNone: MPMovieControlStyle = 0;
/// `MPMovieControlModeHidden`, for `setMovieControlMode:` (iPhone OS 3.1
/// and earlier).
const MPMovieControlModeHidden: NSInteger = 2;

// Values might not be correct, but as these are linked symbol constants, it
// shouldn't matter.
pub const MPMoviePlayerPlaybackDidFinishNotification: &str =
    "MPMoviePlayerPlaybackDidFinishNotification";
/// Apparently an undocumented, private API. Spore Origins uses it.
pub const MPMoviePlayerContentPreloadDidFinishNotification: &str =
    "MPMoviePlayerContentPreloadDidFinishNotification";
pub const MPMoviePlayerScalingModeDidChangeNotification: &str =
    "MPMoviePlayerScalingModeDidChangeNotification";
/// Never posted, since movies don't really play yet.
pub const MPMovieDurationAvailableNotification: &str = "MPMovieDurationAvailableNotification";
// TODO: More notifications?
const MPMoviePlayerPlaybackDidFinishReasonUserInfoKey: &str =
    "MPMoviePlayerPlaybackDidFinishReasonUserInfoKey";

/// `NSNotificationName` values and other constants.
pub const CONSTANTS: ConstantExports = &[
    (
        "_MPMoviePlayerPlaybackDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishNotification),
    ),
    (
        "_MPMoviePlayerContentPreloadDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerContentPreloadDidFinishNotification),
    ),
    (
        "_MPMoviePlayerScalingModeDidChangeNotification",
        HostConstant::NSString(MPMoviePlayerScalingModeDidChangeNotification),
    ),
    (
        "_MPMovieDurationAvailableNotification",
        HostConstant::NSString(MPMovieDurationAvailableNotification),
    ),
    (
        "_MPMoviePlayerPlaybackDidFinishReasonUserInfoKey",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishReasonUserInfoKey),
    ),
];

struct MPMoviePlayerControllerHostObject {
    // NSURL *
    content_url: id,
    movie_source_type: MPMovieSourceType,
    should_autoplay: bool,
    control_style: MPMovieControlStyle,
    movie_control_mode: NSInteger,
    orientation: UIDeviceOrientation,
    use_application_audio_session: bool,
    // UIView *, made when first asked for
    background_view: id,
}
impl HostObject for MPMoviePlayerControllerHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation MPMoviePlayerController: NSObject

// TODO: actual playback

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(MPMoviePlayerControllerHostObject {
        content_url: nil,
        movie_source_type: MPMovieSourceTypeUnknown,
        should_autoplay: true,
        control_style: 1, // MPMovieControlStyleDefault
        movie_control_mode: 0, // MPMovieControlModeDefault
        orientation: default_orientation(env),
        use_application_audio_session: true,
        background_view: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithContentURL:(id)url { // NSURL*
    log!(
        "TODO: [(MPMoviePlayerController*){:?} initWithContentURL:{:?} ({:?})]",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );

    retain(env, url);
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).content_url = url;

    // Act as if loading immediately completed (Spore Origins waits for this).
    State::get(env).pending_notifications.push_back(
        (MPMoviePlayerContentPreloadDidFinishNotification, this, Instant::now())
    );

    this
}

- (())dealloc {
    let host_obj = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this);
    let (url, background_view) = (host_obj.content_url, host_obj.background_view);
    release(env, url);
    release(env, background_view);

    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)contentURL {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).content_url
}

- (id)backgroundColor {
    msg_class![env; UIColor blackColor] // TODO
}
- (())setBackgroundColor:(id)color { // UIColor*
    todo_objc_setter!(this, color);
}

- (())setScalingMode:(MPMovieScalingMode)mode {
    todo_objc_setter!(this, mode);
}
- (bool)useApplicationAudioSession {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).use_application_audio_session
}
- (())setUseApplicationAudioSession:(bool)use_session {
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).use_application_audio_session =
        use_session;
}
- (MPMovieControlStyle)controlStyle {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).control_style
}
- (())setControlStyle:(MPMovieControlStyle)style {
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).control_style = style;
}
- (())setFullscreen:(bool)fullsreen {
    todo_objc_setter!(this, fullsreen);
}
- (bool)shouldAutoplay {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).should_autoplay
}
- (())setShouldAutoplay:(bool)autoplay {
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).should_autoplay = autoplay;
}
- (())setInitialPlaybackTime:(NSTimeInterval)initial_time {
    todo_objc_setter!(this, initial_time);
}

// Whether the content is a file or a stream: only a hint for loading it.
- (MPMovieSourceType)movieSourceType {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).movie_source_type
}
- (())setMovieSourceType:(MPMovieSourceType)source_type {
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).movie_source_type =
        source_type;
}

- (id)view {
    nil // TODO
}

// For the game's own content behind the movie. TODO: it isn't shown, as
// there's no view to put it in yet.
- (id)backgroundView {
    let view = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).background_view;
    if view != nil {
        return view;
    }
    let view: id = msg_class![env; UIView alloc];
    let view: id = msg![env; view init];
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).background_view = view;
    view
}

- (MPMoviePlaybackState)playbackState {
    if State::get(env).native_player == Some(this) {
        MPMoviePlaybackStatePlaying
    } else {
        MPMoviePlaybackStateStopped // TODO
    }
}

// iPhone OS 3.1 and earlier's way to choose the controls.
- (())setMovieControlMode:(NSInteger)mode {
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).movie_control_mode = mode;
}

// Another undocumented one! But some apps may still use it :/
// https://stackoverflow.com/a/1390079/2241008
- (())setOrientation:(UIDeviceOrientation)orientation animated:(bool)_animated {
    if (UIDeviceOrientationPortrait..=UIDeviceOrientationLandscapeRight).contains(&orientation) {
        env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).orientation = orientation;
    }
}

// MPMediaPlayback implementation
- (())prepareToPlay {
    log!("TODO: [(MPMoviePlayerController*){:?} prepareToPlay]", this);
    // Some games never call play, and wait for the movie to finish.
    if env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).should_autoplay {
        () = msg![env; this play];
    }
}

- (())play {
    log!("TODO: [(MPMoviePlayerController*){:?} play]", this);
    let state = State::get(env);
    if state.native_player == Some(this) && state.active_player == Some(this) {
        return; // Already playing.
    }
    if let Some(old) = env.framework_state.media_player.movie_player.active_player {
        let _: () = msg![env; old stop];
    }
    assert!(env.framework_state.media_player.movie_player.active_player.is_none());
    // Movie player is retained by the runtime until it is stopped
    retain(env, this);
    env.framework_state.media_player.movie_player.active_player = Some(this);

    if play_natively(env, this) {
        return;
    }

    // Act as if playback immediately completed after 1 second
    // (various apps wait for this, such as BIA and Hero of Sparta).
    let notif = (MPMoviePlayerPlaybackDidFinishNotification, this, Instant::now().checked_add(Duration::from_millis(1000)).unwrap());
    for (name, obj, _) in &mut State::get(env).pending_notifications {
        // De-duplicate similar notifications. This can happen if app is calling
        // `play` twice on the same player object (case of NOVA2).
        if *name == MPMoviePlayerPlaybackDidFinishNotification && *obj == this {
            return;
        }
    }
    State::get(env).pending_notifications.push_back(notif);
}

- (())pause {
    log!("TODO: [(MPMoviePlayerController*){:?} pause]", this);
}

- (())stop {
    log!("TODO: [(MPMoviePlayerController*){:?} stop]", this);
    if env.framework_state.media_player.movie_player.active_player.is_some() {
        // Some applications (like NOVA2) may send 2 `stop` messages for each
        // 1 `play` message for the player. In that case, we want to release
        // the active player only once.
        assert!(this == env.framework_state.media_player.movie_player.active_player.take().unwrap());
        release(env, this);
    }
    // The finish notification is still posted, as for the faked playback.
    if State::get(env).native_player == Some(this) {
        unsafe { (NATIVE_PLAYER.get().unwrap().stop)() };
    }
}

@end

@implementation MPMoviePlayerViewController: UIViewController

- (id)initWithContentURL:(id)url {
    log!(
        "TODO: [(MPMoviePlayerViewController*){:?} initWithContentURL:{:?} ({:?})] -> nil",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );
    release(env, this);
    nil // TODO
}

@end

};

/// iPhone OS 3.1's player always plays in landscape, home button on the
/// right, so some games' movies are made sideways for it. Games built for
/// 3.2 and later get one that follows the game's orientation.
fn default_orientation(env: &Environment) -> UIDeviceOrientation {
    let version = env.bundle.sdk_version();
    let version = version.or(env.bundle.minimum_os_version()).unwrap_or("");
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let version = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    if version >= (3, 2) {
        UIDeviceOrientationUnknown
    } else {
        UIDeviceOrientationLandscapeLeft
    }
}

/// Has the host play the player's movie, if it can. Returns `false` if it
/// can't, so that playback is faked instead.
fn play_natively(env: &mut Environment, this: id) -> bool {
    let Some(&native) = NATIVE_PLAYER.get() else {
        return false;
    };
    // A stopped movie that hasn't finished closing yet.
    if let Some(old) = State::get(env).native_player.take() {
        State::get(env).pending_notifications.push_back((
            MPMoviePlayerPlaybackDidFinishNotification,
            old,
            Instant::now(),
        ));
        release(env, old);
    }
    let host_obj = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this);
    let url = host_obj.content_url;
    let orientation = host_obj.orientation;
    let controls = host_obj.control_style != MPMovieControlStyleNone
        && host_obj.movie_control_mode != MPMovieControlModeHidden;
    let is_file: bool = msg![env; url isFileURL];
    if !is_file {
        log!("Can't play non-file movie URL {:?}, faking playback", url);
        return false;
    }
    // The game may be read from an IPA, so the host player gets a copy.
    let path = ns_url::to_rust_path(env, url);
    let Ok(bytes) = env.fs.read(&path) else {
        log!("Couldn't read movie {:?}, faking playback", path);
        return false;
    };
    // The extension tells the host player what kind of file it is.
    let extension = path.as_str().rsplit_once('.').map_or("", |(_, e)| e);
    let host_path = std::env::temp_dir().join(format!("Movie.{extension}"));
    if let Err(e) = std::fs::write(&host_path, bytes) {
        log!("Couldn't write {:?} ({}), faking playback", host_path, e);
        return false;
    }
    let host_path = CString::new(host_path.to_str().unwrap()).unwrap();
    unsafe { (native.play)(host_path.as_ptr(), controls, orientation) };
    retain(env, this);
    State::get(env).native_player = Some(this);
    true
}

/// For use by `NSRunLoop` via [super::handle_players]: check movie players'
/// status, send notifications if necessary.
pub(super) fn handle_players(env: &mut Environment) {
    let finished = State::get(env)
        .native_player
        .filter(|_| unsafe { (NATIVE_PLAYER.get().unwrap().finished)() });
    if let Some(player) = finished {
        let state = State::get(env);
        state.native_player = None;
        state.pending_notifications.push_back((
            MPMoviePlayerPlaybackDidFinishNotification,
            player,
            Instant::now(),
        ));
    }

    let mut notifs_to_run = Vec::new();
    let pending_notifs = &mut State::get(env).pending_notifications;
    let mut i = 0;
    while i < pending_notifs.len() {
        let (name_str, object, time) = pending_notifs[i];
        if Instant::now() >= time {
            notifs_to_run.push((name_str, object));
            pending_notifs.swap_remove_back(i);
        } else {
            i += 1;
        }
    }
    for (name_str, object) in notifs_to_run {
        let name = ns_string::get_static_str(env, name_str);
        let center: id = msg_class![env; NSNotificationCenter defaultCenter];
        // TODO: should there be some user info attached?
        let _: () = msg![env; center postNotificationName:name object:object];
    }
    if let Some(player) = finished {
        release(env, player);
    }
}
