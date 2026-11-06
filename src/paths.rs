/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Paths for host files used by touchHLE: settings, fonts, etc.
//!
//! There are three categories of files:
//!
//! * Resources bundled with touchHLE that neither touchHLE nor the user should
//!   modify: [DYLIBS_DIR], [FONTS_DIR], [DEFAULT_OPTIONS_FILE]. Depending on
//!   the platform these may or may not be ordinary files, and must be accessed
//!   through [ResourceFile].
//! * Files the user is expected to modify, but not touchHLE: [apps_dir],
//!   [user_options_file], [wallpaper_files]. These are ordinary files and are
//!   found in [user_data_base_path].
//! * Files that touchHLE will create and modify, and the user may modify if
//!   they want to: [sandbox_dir], [PHOTO_ALBUM_DIR].
//!   These are ordinary files and are found in [user_data_base_path].
//!
//! See also [crate::fs], which provides a virtual filesystem for the guest app
//! and defines path types.

use std::borrow::Cow;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

/// Name of the directory containing ARMv6 dynamic libraries bundled with
/// touchHLE.
pub const DYLIBS_DIR: &str = "dylibs";

/// Name of the directory containing fonts bundled with touchHLE.
/// The iOS app bundle uses its own names, via symlinks in `iphone/`.
#[cfg(not(target_os = "ios"))]
pub const FONTS_DIR: &str = "touchHLE_fonts";
#[cfg(target_os = "ios")]
pub const FONTS_DIR: &str = "Fonts";

/// Name of the file containing touchHLE's default options for various apps.
#[cfg(not(target_os = "ios"))]
pub const DEFAULT_OPTIONS_FILE: &str = "touchHLE_default_options.txt";
#[cfg(target_os = "ios")]
pub const DEFAULT_OPTIONS_FILE: &str = "DefaultOptions.txt";

/// macOS-only: If touchHLE is located in a .app bundle, return the path of the
/// Resources directory. If touchHLE is not located in a .app bundle, return
/// [None].
#[allow(dead_code)]
fn get_macos_bundled_resources_path() -> Option<PathBuf> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let base_path = PathBuf::from(sdl2::filesystem::base_path().ok()?);
    if base_path.file_name().is_some_and(|p| p == "Resources") {
        Some(base_path)
    } else {
        None
    }
}

/// Abstraction over a platform-specific type for accessing a resource bundled
/// with touchHLE.
pub struct ResourceFile {
    inner: ResourceFileInner,
}
enum ResourceFileInner {
    #[cfg(target_os = "android")]
    Asset(sdl2::rwops::RWops<'static>),
    // Embedded, not shipped loose: Apple's validator rejects any Mach-O
    // file it finds in the bundle outside the app's own executable.
    #[cfg(target_os = "ios")]
    Embedded(std::io::Cursor<&'static [u8]>),
    #[cfg(not(target_os = "android"))]
    Disk(std::fs::File),
}
impl Read for ResourceFileInner {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(target_os = "android")]
            Self::Asset(f) => f.read(buf),
            #[cfg(target_os = "ios")]
            Self::Embedded(c) => c.read(buf),
            #[cfg(not(target_os = "android"))]
            Self::Disk(f) => f.read(buf),
        }
    }
}
impl Seek for ResourceFileInner {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match self {
            #[cfg(target_os = "android")]
            Self::Asset(f) => f.seek(pos),
            #[cfg(target_os = "ios")]
            Self::Embedded(c) => c.seek(pos),
            #[cfg(not(target_os = "android"))]
            Self::Disk(f) => f.seek(pos),
        }
    }
}
#[cfg(target_os = "ios")]
fn embedded_dylib_bytes(path: &str) -> Option<&'static [u8]> {
    match path {
        "dylibs/libgcc_s.1.dylib" => Some(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/dylibs/libgcc_s.1.dylib"
            ))
            .as_slice(),
        ),
        "dylibs/libsqlite3.dylib" => Some(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/dylibs/libsqlite3.dylib"
            ))
            .as_slice(),
        ),
        "dylibs/libstdc++.6.0.9.dylib" => Some(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/dylibs/libstdc++.6.0.9.dylib"
            ))
            .as_slice(),
        ),
        "dylibs/libxml2.2.dylib" => Some(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/dylibs/libxml2.2.dylib"
            ))
            .as_slice(),
        ),
        "dylibs/libz.1.2.3.dylib" => Some(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/dylibs/libz.1.2.3.dylib"
            ))
            .as_slice(),
        ),
        _ => None,
    }
}
impl ResourceFile {
    pub fn open(path: &str) -> Result<Self, String> {
        #[cfg(target_os = "ios")]
        if let Some(bytes) = embedded_dylib_bytes(path) {
            return Ok(Self {
                inner: ResourceFileInner::Embedded(std::io::Cursor::new(bytes)),
            });
        }

        Ok(Self {
            inner: {
                // On Android, these resources are included as "assets" within
                // the APK. We access them via SDL2's wrapper of Android's
                // assets API.
                #[cfg(target_os = "android")]
                {
                    ResourceFileInner::Asset(sdl2::rwops::RWops::from_file(path, "r")?)
                }

                // On other OSes (and for non-embedded resources on iOS),
                // resources are accessed as ordinary files.
                #[cfg(not(target_os = "android"))]
                {
                    let base_path = get_macos_bundled_resources_path();
                    // When not in a bundle, look in the current directory.
                    let path = base_path.as_deref().unwrap_or(Path::new(".")).join(path);
                    ResourceFileInner::Disk(std::fs::File::open(path).map_err(|e| e.to_string())?)
                }
            },
        })
    }
    pub fn get(&mut self) -> &mut (impl Read + Seek) {
        &mut self.inner
    }
}
impl std::fmt::Debug for ResourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "ResourceFile")
    }
}

/// Whether various resources are in user-accessible files. If they aren't,
/// touchHLE has to be able to display their license terms.
/// Not on iOS either: users can't open the app bundle.
pub const RESOURCES_ARE_EXTERNAL_FILES: bool =
    cfg!(not(any(target_os = "android", target_os = "ios")));

/// Name of the directory where the user can put apps if they want them to
/// appear in the app picker.
pub fn apps_dir() -> &'static str {
    "Games"
}

/// Name of the file intended for the user's own options.
pub fn user_options_file() -> &'static str {
    "Options.txt"
}

/// Names of files the user can put a wallpaper image (for the app picker) in.
pub fn wallpaper_files() -> &'static [&'static str] {
    &["Wallpaper.png", "Wallpaper.jpg", "Wallpaper.jpeg"]
}

/// Name of the directory where touchHLE will store sandboxed app data, e.g.
/// the `Documents` directory.
pub fn sandbox_dir() -> &'static str {
    "Saves"
}

/// Names of the log file and the previous run's log (see [crate::log]).
pub fn log_files() -> [&'static str; 2] {
    ["Log.txt", "Log.previous.txt"]
}

/// Name of the directory where touchHLE will store IMG_####.PNG files saved to
/// the Photo Album.
pub const PHOTO_ALBUM_DIR: &str = "DCIM/100APPLE";

/// Get a platform-specific base path needed for accessing touchHLE's
/// user-modifiable files. This is empty on platforms other than Android.
pub fn user_data_base_path() -> Cow<'static, Path> {
    #[cfg(target_os = "android")]
    unsafe {
        // This is an exception to the rule that SDL2 should only be used
        // directly from src/window.rs. This is just too distant from windowing
        // to belong there.

        // Android storage has evolved in a quite messy fashion. Both "internal
        // storage" and "external storage" (aka the "SD card") are likely to be
        // internal on a modern device, as absurd as that might sound. SDL2 has
        // APIs to get paths for both. We use the "external storage" because
        // it's more likely to be user-accessible.
        extern "C" {
            fn SDL_AndroidGetExternalStoragePath() -> *const std::ffi::c_char;
        }
        let path = SDL_AndroidGetExternalStoragePath();
        if path.is_null() {
            log!("Couldn't get Android external storage path!");
            panic!();
        }
        Cow::from(Path::new(std::ffi::CStr::from_ptr(path).to_str().unwrap()))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // When touchHLE is run from a .app bundle on macOS, the user might not
        // be able to control the current directory, so user data needs to go in
        // a standard location.
        if get_macos_bundled_resources_path().is_some() {
            static PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
            return Cow::from(PATH.get_or_init(macos_user_data_path).as_path());
        }
        Cow::from(Path::new("."))
    }

    #[cfg(target_os = "ios")]
    {
        // SDL's folder for the app is only used to find Documents.
        let mut pref_path = PathBuf::from(
            sdl2::filesystem::pref_path("idared32bit-emu.com", "iDared 32bit").unwrap(),
        );

        // iDared 32bit
        pref_path.pop();
        // idared32bit-emu.com
        pref_path.pop();
        // Application Support
        pref_path.pop();
        // Older versions used touchHLE's names, and SDL made empty folders
        // for them, which are removed.
        static REMOVE_OLD: std::sync::Once = std::sync::Once::new();
        REMOVE_OLD.call_once(|| {
            let old = pref_path.join("touchhle.org");
            let _ = std::fs::remove_dir(old.join("touchHLE"));
            let _ = std::fs::remove_dir(old);
        });
        // Library
        pref_path.pop();

        pref_path.push("Documents");

        return Cow::from(pref_path);
    }
}

/// Where files that can be rebuilt at any time are kept, out of sight of the
/// user's own files. [None] if there's nowhere suitable.
pub fn cache_dir() -> Option<PathBuf> {
    // Documents' neighbour.
    #[cfg(target_os = "ios")]
    return Some(user_data_base_path().parent()?.join("Library/Caches"));
    #[cfg(not(target_os = "ios"))]
    None
}

/// macOS-only: the user data directory of a .app bundle. Older versions used
/// touchHLE's name for it, and that one is moved here the first time.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn macos_user_data_path() -> PathBuf {
    let new =
        PathBuf::from(sdl2::filesystem::pref_path("idared32bit-emu.com", "iDared 32bit").unwrap());
    // Application Support/touchhle.org/touchHLE
    if let Some(old_org) = new
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("touchhle.org"))
    {
        move_into_empty_dir(&old_org.join("touchHLE"), &new);
        // Only removed if it's now empty.
        let _ = std::fs::remove_dir(old_org);
    }
    new
}

/// Moves `old` to `new` if `new` is an empty directory, which SDL creates when
/// it's asked for the path. Logging isn't set up yet, as the log file goes in
/// this directory, so this prints instead.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn move_into_empty_dir(old: &Path, new: &Path) {
    let new_is_empty = std::fs::read_dir(new).is_ok_and(|mut entries| entries.next().is_none());
    if !old.is_dir() || !new_is_empty || std::fs::remove_dir(new).is_err() {
        return;
    }
    match std::fs::rename(old, new) {
        Ok(()) => eprintln!("Moved {} to {}", old.display(), new.display()),
        Err(e) => {
            eprintln!(
                "Warning: Couldn't move {} to {}: {}",
                old.display(),
                new.display(),
                e
            );
            let _ = std::fs::create_dir_all(new);
        }
    }
}

#[cfg(test)]
#[test]
fn test_move_into_empty_dir() {
    let base = std::env::temp_dir().join(format!("idared-move-test-{}", std::process::id()));
    let (old, new) = (base.join("old"), base.join("new"));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    std::fs::write(old.join("save"), "data").unwrap();
    move_into_empty_dir(&old, &new);
    assert!(!old.exists());
    assert_eq!(std::fs::read_to_string(new.join("save")).unwrap(), "data");
    // Once moved, a new old directory is left alone.
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("other"), "x").unwrap();
    move_into_empty_dir(&old, &new);
    assert!(old.join("other").exists() && !new.join("other").exists());
    std::fs::remove_dir_all(base).unwrap();
}

/// Get a URI that can be used to open a file manager or similar for the path
/// that [user_data_base_path] represents.
pub fn url_for_opening_user_data_dir() -> Result<String, String> {
    if std::env::consts::OS == "android" {
        // See DocumentsProvider.kt, app/build.gradle and AndroidManifest.xml
        let brand = crate::branding();
        Ok(format!(
            "content://org.alexmartin.idared32bit{}{}.provider/root/root",
            if brand.is_empty() { "" } else { "." },
            brand.to_lowercase()
        ))
    } else {
        let path_buf;
        let path = if cfg!(target_os = "ios") {
            user_data_base_path()
        } else {
            path_buf = user_data_base_path()
                .join(".")
                .canonicalize()
                .map_err(|e| format!("Can't canonicalize path to user data directory: {e}"))?;
            Cow::from(path_buf)
        };

        let path_str = path
            .to_str()
            .ok_or_else(|| "User data directory path is not UTF-8".to_string())?;

        if cfg!(target_os = "ios") {
            Ok(format!("shareddocuments://{path_str}"))
        } else {
            // std::fs::canonicalize() on Windows uses the extended-length path
            // syntax, but Windows Explorer doesn't understand it.
            let path_str = if std::env::consts::OS == "windows" {
                path_str.strip_prefix("\\\\?\\").unwrap_or(path_str)
            } else {
                path_str
            };
            Ok(format!("file://{path_str}"))
        }
    }
}

/// Only meaningful on certain OSes: create the user data directory if it
/// doesn't exist, and populate it with templates or README files. (On other
/// platforms these are simply bundled with touchHLE in a ZIP file.)
pub fn prepopulate_user_data_dir() {
    if std::env::consts::OS != "android"
        && std::env::consts::OS != "macos"
        && std::env::consts::OS != "ios"
    {
        return;
    }
    let base_path = user_data_base_path();
    if base_path == Path::new(".") {
        return;
    }

    let apps_dir = base_path.join(apps_dir());
    if !apps_dir.is_dir() {
        match std::fs::create_dir(&apps_dir) {
            Ok(()) => {
                log!("Created: {}", apps_dir.display());
            }
            Err(e) => {
                log!("Warning: Couldn't create {}: {}", apps_dir.display(), e);
            }
        }
    }

    fn create_file(path: &Path, content: &str) {
        match std::fs::write(path, content) {
            Ok(()) => {
                log!("Created: {}", path.display());
            }
            Err(e) => {
                log!("Warning: Couldn't create {}: {}", path.display(), e);
            }
        }
    }

    let apps_dir_readme = apps_dir.join("README.txt");
    let content = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Games/README.txt"));
    // Older versions created it with touchHLE's text.
    let outdated =
        std::fs::read_to_string(&apps_dir_readme).is_ok_and(|s| s == OLD_APPS_DIR_README);
    if !apps_dir_readme.is_file() || outdated {
        create_file(&apps_dir_readme, content);
    }

    let user_options = base_path.join(user_options_file());
    let content = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Options.txt"));
    if !user_options.is_file() {
        create_file(&user_options, content);
    } else if let Ok(old) = std::fs::read_to_string(&user_options) {
        // Older versions created it with this text. Keep the user's lines.
        if let Some(rest) = old.strip_prefix(OLD_USER_OPTIONS_TEMPLATE) {
            create_file(&user_options, &format!("{content}{rest}"));
        }
    }

    // Always the current text, as older versions' text is outdated.
    let options_help = base_path.join("OPTIONS_HELP.txt");
    if std::fs::read_to_string(&options_help).ok().as_deref() != Some(crate::options::OPTIONS_HELP)
    {
        create_file(&options_help, crate::options::OPTIONS_HELP);
    }
}

/// The text older versions created the apps folder's README with.
const OLD_APPS_DIR_README: &str = "If you put your .app bundles or .ipa files \
in this directory, they will show up in the touchHLE app picker.\n";

/// The text older versions created the user's options file with.
const OLD_USER_OPTIONS_TEMPLATE: &str = "\
# This file lets you specify the options you want to use for various apps.
#
# Options in this file take precedence over ones in touchHLE_default_options.txt
#
# ---
#
# Each line in this file specifies options for an app; there should only be one
# line per app. A line should consist of the identifier for an app, followed by
# a colon (:), followed by the options to apply. To find out an app's
# identifier, you can run it in touchHLE and look at the “App bundle info”
# output. To get a list of possible options, see OPTIONS_HELP.txt or run
# touchHLE with the --help flag.
#
# Text on a line following a hash sign (#) is treated as a comment and ignored.
# Empty lines are also ignored.
";

/// Older versions used touchHLE's names for the user's files. Move anything
/// still under an old name to its new one.
/// Nothing is overwritten, so this is safe to run on every launch, and does
/// nothing once it's done. Call it before anything uses the user's files,
/// except the log.
pub fn migrate_old_names() {
    let base_path = user_data_base_path();
    let wallpaper_files = wallpaper_files();
    let names = [
        ("touchHLE_apps", apps_dir()),
        ("touchHLE_sandbox", sandbox_dir()),
        ("touchHLE_options.txt", user_options_file()),
        ("touchHLE_wallpaper.png", wallpaper_files[0]),
        ("touchHLE_wallpaper.jpg", wallpaper_files[1]),
        ("touchHLE_wallpaper.jpeg", wallpaper_files[2]),
        // The new log is already open, so the last run's becomes the previous.
        ("touchHLE_log.txt", log_files()[1]),
    ];
    for (old, new) in names {
        move_without_overwriting(&base_path.join(old), &base_path.join(new));
    }
    // Normal log rotation would have replaced this by now.
    let _ = std::fs::remove_file(base_path.join("touchHLE_log.previous.txt"));
}

/// Moves `from` to `to`. If both are directories, `from`'s contents are moved
/// into `to` one by one, and `from` is removed only if that empties it.
fn move_without_overwriting(from: &Path, to: &Path) {
    if std::fs::symlink_metadata(from).is_err() {
        return;
    }
    if std::fs::symlink_metadata(to).is_err() {
        match std::fs::rename(from, to) {
            Ok(()) => {
                log!("Moved {} to {}", from.display(), to.display());
            }
            Err(e) => {
                log!(
                    "Warning: Couldn't move {} to {}: {}",
                    from.display(),
                    to.display(),
                    e
                );
            }
        }
    } else if from.is_dir() && to.is_dir() {
        for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
            move_without_overwriting(&entry.path(), &to.join(entry.file_name()));
        }
        if std::fs::remove_dir(from).is_ok() {
            log!("Moved {} into {}", from.display(), to.display());
        } else {
            log!(
                "Warning: Some of {} couldn't be moved into {}, so it's left there",
                from.display(),
                to.display()
            );
        }
    } else {
        log!(
            "Warning: Didn't move {}, as {} already exists",
            from.display(),
            to.display()
        );
    }
}
