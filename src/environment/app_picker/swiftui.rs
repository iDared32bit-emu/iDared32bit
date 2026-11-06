/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! On iOS, the app picker is native SwiftUI
//! (`iphone/iDared32bit/AppPicker.swift`) rather than drawn with our own
//! UIKit.

use super::{quick_option_args, quick_options, AppInfo, QuickOptionKind};
use crate::image::Image;
use crate::paths;
use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Shows the picker until an app is picked. Takes JSON describing it and one
/// RGBA icon per app. Returns the picked app's index, then each quick option's
/// value, one per line, in a string to be freed with `free()`.
type PickerFn = unsafe extern "C" fn(*const c_char, *const *const u8, usize) -> *mut c_char;

static PICKER: OnceLock<PickerFn> = OnceLock::new();

/// Called by the iOS app's `main()` with the Swift picker. Rust can't link to
/// it directly, as this crate is also built as a dylib without it.
#[no_mangle]
pub extern "C" fn idared_set_swiftui_app_picker(picker: PickerFn) {
    let _ = PICKER.set(picker);
}

pub fn is_available() -> bool {
    PICKER.get().is_some()
}

pub enum Picked {
    /// The app's path and option arguments.
    App(PathBuf, Vec<String>),
    /// The Games folder changed, so the picker should be shown again.
    Reload(ReloadState),
}

/// What the picker keeps when it's shown again.
pub struct ReloadState {
    page: usize,
    quick_option_values: Vec<usize>,
}

pub fn app_picker(
    apps: Result<Vec<AppInfo>, String>,
    state: Option<ReloadState>,
) -> Result<Picked, String> {
    let picker = *PICKER.get().unwrap();
    let (apps, error) = match apps {
        Ok(apps) => (apps, None),
        Err(e) => (Vec::new(), Some(e)),
    };

    // Swift draws a "?" on this for apps without an icon.
    let mut placeholder = Image::from_pixel_vec([128, 128, 128, 255].repeat(57 * 57), (57, 57));
    // Should match Bundle::load_icon().
    placeholder.round_corners(10.0, /* four_corners: */ true);
    let icons: Vec<&Image> = apps
        .iter()
        .map(|app| app.icon.as_ref().unwrap_or(&placeholder))
        .collect();

    let wallpaper = paths::wallpaper_files()
        .iter()
        .map(|file| paths::user_data_base_path().join(file))
        .find(|path| path.exists());
    let branding = crate::branding();

    let apps_json: Vec<String> = apps
        .iter()
        .zip(&icons)
        .map(|(app, icon)| {
            format!(
                "{{\"name\":{},\"iconWidth\":{},\"iconHeight\":{},\"hasIcon\":{},\
                 \"crashed\":{}}}",
                json_string(&app.display_name),
                icon.dimensions().0,
                icon.dimensions().1,
                app.icon.is_some(),
                app.crashed,
            )
        })
        .collect();
    // Swift shows these, and sends back each one's value: the choice index,
    // or 1 for a switch that's on. Only Rust knows the option arguments.
    let quick_options_json: Vec<String> = quick_options()
        .map(|option| {
            let kind = match option.kind {
                QuickOptionKind::Choice(choices) => {
                    let names: Vec<String> = choices.iter().map(|c| json_string(c.0)).collect();
                    format!("\"choices\":[{}]", names.join(","))
                }
                QuickOptionKind::Switch { .. } => "\"choices\":null".to_owned(),
            };
            format!(
                "{{\"label\":{},\"footer\":{},{},\"defaultValue\":{}}}",
                json_string(option.label),
                json_string(option.footer),
                kind,
                option.default_value(),
            )
        })
        .collect();
    let optional = |s: Option<String>| s.map_or("null".to_owned(), |s| json_string(&s));
    let json = format!(
        "{{\"apps\":[{}],\"error\":{},\"wallpaperPath\":{},\
         \"wallpaperFolder\":{},\"wallpaperFiles\":[{}],\"appsFolder\":{},\
         \"initialPage\":{},\"initialQuickOptionValues\":{},\"branding\":{},\
         \"shortVersionText\":{},\"copyright\":{},\
         \"fileManagerURL\":{},\"websiteURL\":{},\"quickOptions\":[{}],\
         \"startError\":{}}}",
        apps_json.join(","),
        optional(error),
        optional(wallpaper.map(|p| p.to_string_lossy().into_owned())),
        json_string(&paths::user_data_base_path().to_string_lossy()),
        paths::wallpaper_files()
            .iter()
            .map(|file| json_string(file))
            .collect::<Vec<_>>()
            .join(","),
        json_string(
            &paths::user_data_base_path()
                .join(paths::apps_dir())
                .to_string_lossy()
        ),
        state.as_ref().map_or(0, |state| state.page),
        state.as_ref().map_or("null".to_owned(), |state| {
            let values: Vec<String> = state
                .quick_option_values
                .iter()
                .map(usize::to_string)
                .collect();
            format!("[{}]", values.join(","))
        }),
        json_string(branding),
        json_string(crate::DISPLAY_VERSION),
        json_string(&crate::licenses::get_text()),
        optional(paths::url_for_opening_user_data_dir().ok()),
        json_string("https://iDared32bit-emu.com/"),
        quick_options_json.join(","),
        optional(crate::window::START_ERROR.lock().unwrap().take()),
    );
    // Control characters, including NUL, are escaped above.
    let json = CString::new(json).unwrap();

    let icon_ptrs: Vec<*const u8> = icons.iter().map(|icon| icon.pixels().as_ptr()).collect();
    let result_ptr = unsafe { picker(json.as_ptr(), icon_ptrs.as_ptr(), icon_ptrs.len()) };
    if result_ptr.is_null() {
        return Err("Couldn't show the app picker.".to_string());
    }
    let result = unsafe { CStr::from_ptr(result_ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { libc::free(result_ptr.cast()) };

    let mut lines = result.lines();
    if result.starts_with("reload\n") {
        lines.next();
        let page = lines.next().and_then(|page| page.parse().ok()).unwrap_or(0);
        let quick_option_values = lines.map(|value| value.parse().unwrap_or(0)).collect();
        echo!("The Games folder changed, so the app picker is shown again.");
        return Ok(Picked::Reload(ReloadState {
            page,
            quick_option_values,
        }));
    }
    let app = lines
        .next()
        .and_then(|idx| idx.parse::<usize>().ok())
        .and_then(|idx| apps.get(idx))
        .ok_or_else(|| "The app picker returned an invalid app.".to_string())?;
    let values: Vec<usize> = lines.map(|value| value.parse().unwrap_or(0)).collect();
    echo!("Picked: {}", app.path.display());
    Ok(Picked::App(
        app.path.clone(),
        quick_option_args(&values).collect(),
    ))
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
