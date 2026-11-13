/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSBundle`.

use super::{ns_data, ns_string, NSUInteger};
use crate::bundle::Bundle;
use crate::frameworks::core_foundation::cf_bundle::{
    CFBundleCopyBundleLocalizations, CFBundleCopyPreferredLocalizationsFromArray,
};
use crate::frameworks::foundation::ns_string::{from_rust_string, to_rust_string};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, Class, ClassExports,
    HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::{HashMap, HashSet};

use crate::fs::{FsNodeType, GuestPath};

// Should be ISO 639-1 (or ISO 639-2) compliant
// Legacy projects use language names while newer ones use language code lprojs
// TODO: complete this list or use some crate for mapping
pub const LANG_ID_TO_LANG_PROJ: &[(&str, &[&str])] = &[
    ("da", &["Danish.lproj", "da.lproj"]),
    ("nl", &["Dutch.lproj", "nl.lproj"]),
    ("en", &["English.lproj", "en.lproj"]),
    ("fi", &["Finnish.lproj", "fi.lproj"]),
    ("fr", &["French.lproj", "fr.lproj"]),
    ("de", &["German.lproj", "de.lproj"]),
    ("it", &["Italian.lproj", "it.lproj"]),
    ("ja", &["Japanese.lproj", "ja.lproj"]),
    ("no", &["Norwegian.lproj", "no.lproj"]),
    ("es", &["Spanish.lproj", "es.lproj"]),
    ("sv", &["Swedish.lproj", "sv.lproj"]),
];

#[derive(Default)]
pub struct State {
    main_bundle: Option<id>,
    localization_tables: HashMap<id, id>, // NSString* to NSDictionary*
}

pub struct NSBundleHostObject {
    /// If this is [None], this is the main bundle's NSBundle instance and the
    /// [Bundle] is stored in [crate::Environment], not here.
    pub bundle: Option<Bundle>,
    /// NSString with bundle path.
    bundle_path: id,
    /// NSString with bundle identifier.
    bundle_identifier: id,
    /// NSURL with bundle path. [None] if not created yet.
    bundle_url: Option<id>,
    /// `NSDictionary*` for the `Info.plist` content. [None] if not created yet.
    info_dictionary: Option<id>,
}
impl HostObject for NSBundleHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSBundle: NSObject

+ (id)mainBundle {
    if let Some(bundle) = env.framework_state.foundation.ns_bundle.main_bundle {
        bundle
    } else {
        let new = msg_class![env; _touchHLE_NSBundle_Static alloc];
        env.framework_state.foundation.ns_bundle.main_bundle = Some(new);
        new
   }
}

// Only the app's own bundle exists here, not the system frameworks' ones.
+ (id)bundleWithIdentifier:(id)identifier { // NSString*
    let main_bundle: id = msg![env; this mainBundle];
    let main_id: id = msg![env; main_bundle bundleIdentifier];
    if identifier != nil && msg![env; identifier isEqualToString:main_id] {
        return main_bundle;
    }
    log!(
        "Warning: [NSBundle bundleWithIdentifier:{:?}] returning nil, only the app's own bundle is available",
        if identifier == nil { "nil".into() } else { ns_string::to_rust_string(env, identifier) }
    );
    nil
}

+ (id)pathForResource:(id)name // NSString*
               ofType:(id)extension // NSString*
          inDirectory:(id)bundle_path { // NSString*
    // A throwaway bundle for the path, to search it like any other.
    let host_object = NSBundleHostObject {
        bundle: None,
        bundle_path,
        bundle_identifier: nil,
        bundle_url: None,
        info_dictionary: None,
    };
    let bundle = env.objc.alloc_object(this, Box::new(host_object), &mut env.mem);
    let path: id = msg![env; bundle pathForResource:name ofType:extension inDirectory:nil];
    release(env, bundle);
    path
}

+ (id)preferredLocalizationsFromArray:(id)localizations_array { // NSArray<NSString *> *
    let preferredLocalizations = CFBundleCopyPreferredLocalizationsFromArray(env, localizations_array);
    autorelease(env, preferredLocalizations)
}

- (())dealloc {
    let &NSBundleHostObject {
        bundle: _,
        bundle_path: _, // FIXME?
        bundle_identifier: _, // FIXME?
        bundle_url,
        info_dictionary,
    } = env.objc.borrow(this);
    if let Some(bundle_url) = bundle_url {
        release(env, bundle_url);
    }
    if let Some(info_dictionary) = info_dictionary {
        release(env, info_dictionary);
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)bundlePath {
    env.objc.borrow::<NSBundleHostObject>(this).bundle_path
}
- (id)bundleIdentifier {
    env.objc.borrow::<NSBundleHostObject>(this).bundle_identifier
}
- (id)bundleURL {
    if let Some(url) = env.objc.borrow::<NSBundleHostObject>(this).bundle_url {
        url
    } else {
        let bundle_path: id = msg![env; this bundlePath];
        let new: id = msg_class![env; NSURL alloc];
        let new: id = msg![env; new initFileURLWithPath:bundle_path];
        env.objc.borrow_mut::<NSBundleHostObject>(this).bundle_url = Some(new);
        new
    }
}

- (id)loadNibNamed:(id)name // NSString*
             owner:(id)owner
           options:(id)options { // NSDictionary<UINibOptionsKey, id> *
    if options != nil {
        let options_count: NSUInteger = msg![env; options count];
        // TODO: support options
        assert_eq!(options_count, 0);
    }

    let nib : id = msg_class![env; UINib nibWithNibName:name bundle:this];
    let arr = msg![env; nib instantiateWithOwner:owner options:nil];

    // Need to filter out the File's Owner or any proxy objects
    let ui_proxy_class: Class = msg_class![env; UIProxyObject class];
    let res = msg_class![env; NSMutableArray new];
    let count: NSUInteger = msg![env; arr count];
    for i in 0..count {
        let curr_object: id = msg![env; arr objectAtIndex:i];
        if curr_object == owner || msg![env; curr_object isKindOfClass:ui_proxy_class] {
            continue;
        }
        () = msg![env; res addObject:curr_object];
    }
    let res_imm = msg![env; res copy];
    release(env, res);
    autorelease(env, res_imm)
}

- (id)resourcePath {
    // This seems to be the same as the bundle path. The iPhone OS bundle
    // structure is a lot flatter than the macOS one.
    msg![env; this bundlePath]
}
- (id)resourceURL {
    // This seems to be the same as the bundle path. The iPhone OS bundle
    // structure is a lot flatter than the macOS one.
    msg![env; this bundleURL]
}

- (id)executablePath {
    let exec_path_str = env.bundle.executable_path().as_str().to_string();
    let exec_path = from_rust_string(env, exec_path_str);
    autorelease(env, exec_path)
}
- (id)executableURL {
    // TODO: cache result
    let exec_path: id = msg![env; this executablePath];
    msg_class![env; NSURL fileURLWithPath:exec_path]
}

- (id)pathForResource:(id)name // NSString*
               ofType:(id)extension // NSString*
          inDirectory:(id)directory // NSString*
      forLocalization:(id)localizationName { // NSString*
    if localizationName == nil {
        return msg![env; this pathForResource:name ofType:extension inDirectory:directory];
    }
    assert!(name != nil); // TODO

    // TODO: cache result of lookups

    let path = path_for_resource_helper(env, this, name, nil, directory, extension);
    if path != nil {
        return path;
    }

    // The localization is an .lproj folder's name, e.g. "fr" or "French".
    // Bundles use either for the same language, so both are tried.
    let lang = ns_string::to_rust_string(env, localizationName);
    let first = format!("{lang}.lproj");
    let mut lprojs = vec![first.clone()];
    if let Some(&(_, names)) = LANG_ID_TO_LANG_PROJ
        .iter()
        .find(|&&(code, names)| code == lang || names.contains(&first.as_str()))
    {
        lprojs.extend(names.iter().filter(|&&n| n != first).map(|&n| n.to_string()));
    }
    for lproj in lprojs {
        let lproj = ns_string::from_rust_string(env, lproj);
        let path = path_for_resource_helper(env, this, name, lproj, directory, extension);
        release(env, lproj);
        if path != nil {
            return path;
        }
    }
    nil
}
- (id)pathForResource:(id)name // NSString*
               ofType:(id)extension // NSString*
          inDirectory:(id)directory { // NSString*
    let is_empty = |env: &mut Environment, s: id| {
        s == nil || ns_string::to_rust_string(env, s).is_empty()
    };
    // As in Core Foundation: no name means the first file of the type, and
    // no type means any type.
    if is_empty(env, name) {
        let paths: id = msg![env; this pathsForResourcesOfType:extension inDirectory:directory];
        let count: NSUInteger = msg![env; paths count];
        let mut first: Option<(String, id)> = None;
        for i in 0..count {
            let path: id = msg![env; paths objectAtIndex:i];
            let path_str = ns_string::to_rust_string(env, path).into_owned();
            if first.as_ref().is_none_or(|(f, _)| path_str < *f) {
                first = Some((path_str, path));
            }
        }
        return first.map_or(nil, |(_, path)| path);
    }

    // TODO: cache result of lookups

    let path = path_for_resource_helper(env, this, name, nil, directory, extension);
    if path != nil {
        return path
    }

    // Try preferred languages in order of preference
    // TODO: Support both Region-specific and Language-specific
    // localized resources
    let langs: id = msg_class![env; NSLocale preferredLanguages];
    let lang_count: NSUInteger = msg![env; langs count];
    let mut unknown_codes = HashSet::new();
    for i in 0..lang_count {
        let lang_code: id = msg![env; langs objectAtIndex:i];
        let lang_code = ns_string::to_rust_string(env, lang_code); // TODO: avoid copy
        if let Some(&(_, lprojs)) = LANG_ID_TO_LANG_PROJ.iter().find(|&&(code, _)| code == lang_code) {
            for lproj in lprojs {
                let lproj: id = ns_string::get_static_str(env, lproj);
                let localized_path = path_for_resource_helper(env, this, name, lproj, directory, extension);
                if localized_path != nil {
                    return localized_path;
                }
            }
        } else {
            unknown_codes.insert(lang_code);
        }
    }

    // TODO: Support look up for device specific resources, e.g. ~iphone

    // As a last resort, fallback to English
    // TODO: fallback to a development language (CFBundleDevelopmentRegion from
    // Info.plist)
    if !unknown_codes.is_empty() {
        log!("TODO: language codes {:?} aren't mapped to a language name, falling back to English", unknown_codes);
    }

    for lproj in ["English.lproj", "en.lproj"] {
        let lproj: id = ns_string::get_static_str(env, lproj);
        let path = path_for_resource_helper(env, this, name, lproj, directory, extension);
        if path != nil {
            return path;
        }
    }
    nil
}
- (id)pathForResource:(id)name // NSString*
               ofType:(id)extension { // NSString*
    msg![env; this pathForResource:name ofType:extension inDirectory:nil]
}
- (id)URLForResource:(id)name // NSString*
       withExtension:(id)extension // NSString *
        subdirectory:(id)subpath { // NSString *
    let path_string: id = msg![env; this pathForResource:name
                                                 ofType:extension
                                            inDirectory:subpath];
    if path_string == nil {
        return nil;
    }
    let path_url: id = msg_class![env; NSURL alloc];
    let path_url: id = msg![env; path_url initFileURLWithPath:path_string];
    autorelease(env, path_url)
}
- (id)URLForResource:(id)name // NSString*
       withExtension:(id)extension { // NSString *
    msg![env; this URLForResource:name withExtension:extension subdirectory:nil]
}

- (id)localizedStringForKey:(id)key
                      value:(id)value
                      table:(id)table_name {
    log_dbg!("localizedStringForKey key:'{}' value:'{}' table:'{}'",
            if key == nil { std::borrow::Cow::from("(null)") } else { ns_string::to_rust_string(env, key) },
            if value == nil { std::borrow::Cow::from("(null)") } else { ns_string::to_rust_string(env, value) },
            if table_name == nil { std::borrow::Cow::from("(null)") } else { ns_string::to_rust_string(env, table_name) }
    );
    let empty_str: id = ns_string::get_static_str(env, "");
    if key == nil {
        if value == nil {
            return empty_str;
        }
        return value;
    }
    let name = if table_name == nil || msg![env; table_name isEqualToString:empty_str] {
        ns_string::get_static_str(env, "Localizable")
    } else {
        table_name
    };
    // TODO: support arbitrary bundles, not only main one
    assert_eq!(this, env.framework_state.foundation.ns_bundle.main_bundle.unwrap());
    let dict = if let Some(&table_dict) = env.framework_state.foundation.ns_bundle.localization_tables.get(&name) {
        table_dict
    } else {
        let extension = ns_string::get_static_str(env, "strings");
        let dict_url: id = msg![env; this URLForResource:name withExtension:extension];
        if dict_url == nil {
            log!("Warning: Unable to locate localization table named '{}', caching as nil", to_rust_string(env, name));
            retain(env, name);
            env.framework_state.foundation.ns_bundle.localization_tables.insert(name, nil);
            nil
        } else {
            let dict = {
                // First, try to load as property list format
                let dict: id = msg_class![env; NSDictionary dictionaryWithContentsOfURL:dict_url];
                if dict != nil {
                    dict
                } else {
                    // Else, load as standard format
                    load_strings_as_standard_format(env, dict_url)
                }
            };
            retain(env, name);
            retain(env, dict);
            env.framework_state.foundation.ns_bundle.localization_tables.insert(name, dict);
            dict
        }
    };
    let res: id = msg![env; dict objectForKey:key];
    if res == nil {
        if value == nil || msg![env; value isEqualToString:empty_str] {
            return key;
        }
        return value;
    }
    log_dbg!("localizedStringForKey res => {:?}", ns_string::to_rust_string(env, res));
    res
}

- (id)infoDictionary {
    let &NSBundleHostObject {
        bundle_path,
        info_dictionary,
        ..
    } = env.objc.borrow(this);
    if let Some(dict) = info_dictionary {
        return dict;
    }

    let plist_path = ns_string::get_static_str(env, "Info.plist");
    let plist_path: id = msg![env; bundle_path stringByAppendingPathComponent:plist_path];
    let dict: id = msg_class![env; NSDictionary alloc];
    let dict: id = msg![env; dict initWithContentsOfFile:plist_path];
    env.objc.borrow_mut::<NSBundleHostObject>(this).info_dictionary = Some(dict);
    dict
}

- (id)objectForInfoDictionaryKey:(id)key {
    let info_dict = msg![env; this infoDictionary];
    // TODO: return the localized value of a key when one is available
    msg![env; info_dict objectForKey:key]
}

- (id)localizations {
    let localizations = CFBundleCopyBundleLocalizations(env, this);
    autorelease(env, localizations)
}

- (id)preferredLocalizations {
    let loc_array = CFBundleCopyBundleLocalizations(env, this);

    let preferred_localizations = CFBundleCopyPreferredLocalizationsFromArray(env, loc_array);
    autorelease(env, preferred_localizations)
}

// https://developer.apple.com/documentation/foundation/bundle/paths(forresourcesoftype:indirectory:)-swift.method?language=objc
- (id)pathsForResourcesOfType:(id)extension
                  inDirectory:(id)subpath {
    let resource_path: id = msg![env; this resourcePath];
    let resource_path = ns_string::to_rust_string(env, resource_path).into_owned();
    let sub = if subpath != nil {
        ns_string::to_rust_string(env, subpath).into_owned()
    } else {
        String::new()
    };
    // As on iOS, the current language's resources are listed too.
    let mut dir_paths = vec![resource_path.clone()];
    if let Some(lproj) = preferred_lproj(env, &resource_path) {
        dir_paths.push(format!("{resource_path}/{lproj}"));
    }
    if !sub.is_empty() {
        for dir_path in &mut dir_paths {
            dir_path.push('/');
            dir_path.push_str(&sub);
        }
    }

    let target_ext = if extension != nil {
        let ext_str = ns_string::to_rust_string(env, extension);
        let ext_trimmed = ext_str.trim_start_matches('.').to_lowercase();

        if ext_trimmed.is_empty() {
            None
        } else {
            Some(ext_trimmed)
        }
    } else {
        None
    };

    let mut found_paths: Vec<String> = Vec::new();
    let mut read_any = false;
    for dir_path in &dir_paths {
        let Ok(entries) = env.fs.enumerate_with_types(GuestPath::new(dir_path)) else {
            continue;
        };
        read_any = true;
        for (name, node_type) in entries {
            if matches!(node_type, FsNodeType::File) {
                let keep = match &target_ext {
                    Some(ext) => name.to_lowercase().ends_with(&format!(".{}", ext)),
                    None => true,
                };
                if keep {
                    let clean_path = format!("{}/{}", dir_path, name).replace("//", "/");
                    found_paths.push(clean_path);
                }
            }
        }
    }
    if !read_any {
        log!("Failed to read guest directories {:?}", dir_paths);
    }

    let array: id = msg_class![env; NSMutableArray new];
    for path_str in &found_paths {
        let path_ns: id = ns_string::from_rust_string(env, path_str.to_string());
        let path_ns = autorelease(env, path_ns);
        let _: () = msg![env; array addObject:path_ns];
    }

    log_dbg!("[(NSBundle*) {:?} pathsForResourcesOfType:{:?} inDirectory:{:?}] found {} files", this, extension, subpath, found_paths.len());

    autorelease(env, array)
}

// TODO: constructors, more accessors

@end

// Private static implementation of NSBundle, used for the main bundle
// allocation. This is needed because some apps (e.g. Ovenbreak)
// attempts to release it.
@implementation _touchHLE_NSBundle_Static: NSBundle

+ (id)allocWithZone:(NSZonePtr)_zone {
    let bundle_path = env.bundle.bundle_path().as_str().to_string();
    let bundle_path = ns_string::from_rust_string(env, bundle_path);
    let bundle_identifier = env.bundle.bundle_identifier().to_string();
    let bundle_identifier = ns_string::from_rust_string(env, bundle_identifier);
    let host_object = NSBundleHostObject {
        bundle: None,
        bundle_path,
        bundle_identifier,
        bundle_url: None,
        info_dictionary: None,
    };
    env.objc.alloc_object(
        this,
        Box::new(host_object),
        &mut env.mem
    )
}

- (id) retain { this }
- (()) release {}
- (id) autorelease { this }

@end

};

/// The `.lproj` folder of the language resources are looked up in first,
/// like `pathForResource:ofType:inDirectory:` does.
fn preferred_lproj(env: &mut Environment, resource_path: &str) -> Option<&'static str> {
    let langs: id = msg_class![env; NSLocale preferredLanguages];
    let lang_count: NSUInteger = msg![env; langs count];
    let mut candidates = Vec::new();
    for i in 0..lang_count {
        let lang_code: id = msg![env; langs objectAtIndex:i];
        let lang_code = ns_string::to_rust_string(env, lang_code);
        if let Some(&(_, lprojs)) = LANG_ID_TO_LANG_PROJ
            .iter()
            .find(|&&(code, _)| code == lang_code)
        {
            candidates.extend(lprojs);
        }
    }
    candidates.extend(["English.lproj", "en.lproj"]);
    candidates.into_iter().find(|lproj| {
        env.fs
            .is_dir(GuestPath::new(&format!("{resource_path}/{lproj}")))
    })
}

fn path_for_resource_helper(
    env: &mut Environment,
    bundle: id,
    name: id,
    lproj: id,
    directory: id,
    extension: id,
) -> id {
    let mut path: id = msg![env; bundle resourcePath];
    if lproj != nil {
        path = msg![env; path stringByAppendingPathComponent:lproj];
    }
    if directory != nil {
        path = msg![env; path stringByAppendingPathComponent:directory];
    }
    path = msg![env; path stringByAppendingPathComponent:name];
    if extension != nil {
        path = msg![env; path stringByAppendingPathExtension:extension];
    }
    let file_manager: id = msg_class![env; NSFileManager defaultManager];
    let file_exists: bool = msg![env; file_manager fileExistsAtPath:path];
    if file_exists {
        return path;
    }
    nil
}

/// Helper function which loads a `strings` file from an `dict_url` and parses
/// it as standard format - one or more key-value pairs along with optional
/// comments. Returned dictionary is autoreleased, so it's a responsibility of
/// the caller to retain it.
/// [String Resources reference](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/LoadingResources/Strings/Strings.html#//apple_ref/doc/uid/10000051i-CH6)
fn load_strings_as_standard_format(env: &mut Environment, dict_url: id) -> id {
    let res: id = msg_class![env; NSMutableDictionary new];
    let data: id = msg_class![env; NSData dataWithContentsOfURL:dict_url];
    assert!(data != nil); // TODO
    let pairs = decode_strings_file(ns_data::to_rust_slice(env, data))
        .and_then(|text| parse_strings_file(&text));
    // Like on iOS, a file that can't be parsed gives an empty table, so
    // lookups return their key.
    let pairs = pairs.unwrap_or_else(|e| {
        let path: id = msg![env; dict_url path];
        log!("Warning: Can't parse {}: {}", to_rust_string(env, path), e);
        Vec::new()
    });
    for (key, val) in pairs {
        log_dbg!("Parsed strings: '{}' -> '{}'", key, val);
        let key = from_rust_string(env, key);
        let val = from_rust_string(env, val);
        () = msg![env; res setObject:val forKey:key];
        release(env, key);
        release(env, val);
    }

    let res_imm = msg![env; res copy];
    release(env, res);
    autorelease(env, res_imm)
}

fn decode_strings_file(bytes: &[u8]) -> Result<String, String> {
    let utf16 = |bytes: &[u8], from: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| from(c)).collect();
        String::from_utf16(&units).map_err(|_| "invalid UTF-16".to_string())
    };
    match bytes {
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] | rest => std::str::from_utf8(rest)
            .map(str::to_string)
            .map_err(|_| "invalid UTF-8".to_string()),
    }
}

/// Parses `"key" = "value";` pairs. Keys and values may be unquoted if they
/// only use letters, digits and `_$+/:.-`; `"key";` maps a key to itself.
fn parse_strings_file(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut chars = text.chars().peekable();
    let mut pairs = Vec::new();
    loop {
        skip_whitespace_and_comments(&mut chars)?;
        if chars.peek().is_none() {
            return Ok(pairs);
        }
        let key = parse_string_token(&mut chars)?;
        skip_whitespace_and_comments(&mut chars)?;
        let val = match chars.next() {
            Some(';') => key.clone(),
            Some('=') => {
                skip_whitespace_and_comments(&mut chars)?;
                let val = parse_string_token(&mut chars)?;
                skip_whitespace_and_comments(&mut chars)?;
                if chars.next() != Some(';') {
                    return Err(format!("missing ';' after {key:?}"));
                }
                val
            }
            _ => return Err(format!("missing '=' after {key:?}")),
        };
        pairs.push((key, val));
    }
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

fn skip_whitespace_and_comments(chars: &mut Chars) -> Result<(), String> {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c != '/' {
            break;
        }
        let mut lookahead = chars.clone();
        lookahead.next();
        match lookahead.next() {
            Some('/') => {
                for c in chars.by_ref() {
                    if c == '\n' || c == '\r' {
                        break;
                    }
                }
            }
            Some('*') => {
                chars.nth(1);
                let mut prev = '\0';
                loop {
                    match chars.next() {
                        Some('/') if prev == '*' => break,
                        Some(c) => prev = c,
                        None => return Err("unclosed comment".to_string()),
                    }
                }
            }
            // An unquoted string starting with '/'.
            _ => break,
        }
    }
    Ok(())
}

fn is_unquoted_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_$+/:.-".contains(c)
}

fn parse_string_token(chars: &mut Chars) -> Result<String, String> {
    let mut res = String::new();
    match chars.peek() {
        Some('"') => {
            chars.next();
        }
        Some(&c) if is_unquoted_char(c) => {
            while let Some(c) = chars.next_if(|&c| is_unquoted_char(c)) {
                res.push(c);
            }
            return Ok(res);
        }
        c => return Err(format!("unexpected {c:?} where a string should be")),
    }
    loop {
        let c = match chars.next() {
            Some('"') => return Ok(res),
            Some('\\') => match chars.next() {
                Some('a') => '\x07',
                Some('b') => '\x08',
                Some('f') => '\x0C',
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('v') => '\x0B',
                Some('U') | Some('u') => {
                    let mut unit = 0;
                    for _ in 0..4 {
                        let Some(d) = chars.next_if(char::is_ascii_hexdigit) else {
                            break;
                        };
                        unit = unit * 16 + d.to_digit(16).unwrap();
                    }
                    // Surrogate pairs come as two escapes.
                    if (0xD800..0xDC00).contains(&unit) {
                        let mut lookahead = chars.clone();
                        let mut low = 0;
                        if lookahead.next() == Some('\\')
                            && matches!(lookahead.next(), Some('U' | 'u'))
                        {
                            for _ in 0..4 {
                                let Some(d) = lookahead.next_if(char::is_ascii_hexdigit) else {
                                    break;
                                };
                                low = low * 16 + d.to_digit(16).unwrap();
                            }
                        }
                        if (0xDC00..0xE000).contains(&low) {
                            *chars = lookahead;
                            unit = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                        }
                    }
                    char::from_u32(unit).unwrap_or('\u{FFFD}')
                }
                Some(d @ '0'..='7') => {
                    let mut code = d.to_digit(8).unwrap();
                    for _ in 0..2 {
                        let Some(d) = chars.next_if(|c| ('0'..='7').contains(c)) else {
                            break;
                        };
                        code = code * 8 + d.to_digit(8).unwrap();
                    }
                    char::from_u32(code).unwrap()
                }
                // Any other escaped character (", ', \\, newline) is itself.
                Some(c) => c,
                None => return Err("unclosed string".to_string()),
            },
            Some(c) => c,
            None => return Err("unclosed string".to_string()),
        };
        res.push(c);
    }
}
