/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSURL`.

use super::ns_string::{from_rust_string, get_static_str, to_rust_string};
use super::NSUInteger;
use crate::fs::{GuestPath, GuestPathBuf};
use crate::mem::MutPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;
use std::borrow::Cow;

/// It seems like there's two kinds of NSURLs: ones for file paths, and others.
/// So far only the former is implemented (TODO).
enum NSURLHostObject {
    /// This is a file URL. The NSString is a system path (no `file:///`).
    ///
    /// This is a wrapper around NSString so that conversions between NSURL
    /// and NSString, which happen often, can be simple and efficient.
    FileURL {
        ns_string: id,
        // Relative file URL save the working directory at the time of creation
        // At the moment, used in the description selector.
        working_directory: GuestPathBuf,
    },
    /// Non-file URL.
    OtherURL { ns_string: id },
}
impl HostObject for NSURLHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSURL: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSURLHostObject::FileURL { ns_string: nil, working_directory: env.fs.working_directory().into() };
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

+ (id)URLWithString:(id)url { // NSString*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithString:url];
    autorelease(env, new)
}

+ (id)URLWithString:(id)url // NSString*
      relativeToURL:(id)base_url { // NSURL*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithString:url relativeToURL:base_url];
    autorelease(env, new)
}

+ (id)fileURLWithPath:(id)path { // NSString*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initFileURLWithPath:path];
    autorelease(env, new)
}

+ (id)fileURLWithPath:(id)path // NSString*
          isDirectory:(bool)is_dir {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initFileURLWithPath:path isDirectory:is_dir];
    autorelease(env, new)
}

- (())dealloc {
    match *env.objc.borrow(this) {
        NSURLHostObject::FileURL { ns_string, .. } => release(env, ns_string),
        NSURLHostObject::OtherURL { ns_string } => release(env, ns_string),
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

- (id)initFileURLWithPath:(id)path { // NSString*
    // FIXME: this should guess whether the path is a directory
    msg![env; this initFileURLWithPath:path isDirectory:false]
}

- (id)initFileURLWithPath:(id)path // NSString*
              isDirectory:(bool)_is_dir {
    // FIXME: this does not resolve relative paths to be absolute!
    // TODO: this does not strip the file:/// prefix!
    assert!(!to_rust_string(env, path).starts_with("file:"));
    let path = msg![env; path stringByExpandingTildeInPath];
    let path: id = msg![env; path copy];
    *env.objc.borrow_mut(this) = NSURLHostObject::FileURL { ns_string: path, working_directory: env.fs.working_directory().into() };
    this
}

- (id)initWithString:(id)url { // NSString*
    if url == nil {
        return nil;
    }

    // FIXME: this should parse the URL
    assert!(!to_rust_string(env, url).starts_with("file:")); // TODO
    let url: id = msg![env; url copy];
    *env.objc.borrow_mut(this) = NSURLHostObject::OtherURL { ns_string: url };
    this
}

- (id)initWithString:(id)url // NSString*
       relativeToURL:(id)base_url { // NSURL*
    if url == nil || base_url == nil {
        return msg![env; this initWithString:url];
    }

    // TODO: Keep the relative part and the base separate, for -baseURL,
    // -relativeString and -description. This stores the resolved URL.
    let base = match *env.objc.borrow(base_url) {
        NSURLHostObject::FileURL { ns_string, ref working_directory } => {
            let working_directory = working_directory.as_str().to_string();
            let path = to_rust_string(env, ns_string);
            if path.starts_with('/') {
                format!("file://{}", path)
            } else {
                let path = path.trim_start_matches("./");
                format!("file://{}/{}", working_directory, path)
            }
        },
        NSURLHostObject::OtherURL { ns_string } => {
            to_rust_string(env, ns_string).to_string()
        },
    };
    let reference = to_rust_string(env, url);
    let resolved = resolve_url_reference(&base, &reference);
    let host_object = match resolved.strip_prefix("file://") {
        Some(path) if !path.contains(['?', '#']) => {
            let ns_string = from_rust_string(env, path.to_string());
            NSURLHostObject::FileURL {
                ns_string,
                working_directory: env.fs.working_directory().into(),
            }
        },
        _ => NSURLHostObject::OtherURL {
            ns_string: from_rust_string(env, resolved),
        },
    };
    *env.objc.borrow_mut(this) = host_object;
    this
}

- (bool)isFileURL {
    match env.objc.borrow(this) {
        NSURLHostObject::FileURL { .. } => true,
        NSURLHostObject::OtherURL { .. } => false,
    }
}

- (id)description {
    match env.objc.borrow(this) {
        NSURLHostObject::FileURL { ns_string, working_directory } => {
            let working_directory = working_directory.as_str().to_string();
            let mut description = to_rust_string(env, *ns_string).to_string().clone();
            if !description.starts_with('/') {
                description = format!("{} -- file://localhost{}", description.trim_start_matches("./"), working_directory );
            }
            let desc = from_rust_string(env, description);
            autorelease(env, desc)
        },
        NSURLHostObject::OtherURL { ns_string } => *ns_string,
    }
}

- (id)path {
    match *env.objc.borrow(this) {
        NSURLHostObject::FileURL { ns_string, .. } => ns_string,
        NSURLHostObject::OtherURL { ns_string } => {
            // TODO: Support full URLs, not only ones that are just a path.
            // FIXME: This should do unescaping.
            // TODO: Avoid copy.
            assert!(to_rust_string(env, ns_string).starts_with('/'));
            ns_string
        },
    }
}

- (id)absoluteString {
    match *env.objc.borrow(this) {
        // FIXME: don't assume URL is already absolute
        NSURLHostObject::FileURL { ns_string, .. } => ns_string,
        NSURLHostObject::OtherURL { ns_string } => {
            // TODO: full RFC 1808 resolution
            assert!(to_rust_string(env, ns_string).starts_with("http"));
            ns_string
        },
    }
}

- (id)query {
    // File URLs made from paths never have one.
    let &NSURLHostObject::OtherURL { ns_string } = env.objc.borrow(this) else {
        return nil;
    };
    let url = to_rust_string(env, ns_string).to_string();
    // Still percent-escaped, as on iOS.
    let Some(query) = split_url(&url).3 else {
        return nil;
    };
    let query = from_rust_string(env, query.to_string());
    autorelease(env, query)
}

- (id)absoluteURL {
    // FIXME: don't assume URL is already absolute
    let &NSURLHostObject::OtherURL { .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    this
}

- (bool)getFileSystemRepresentation:(MutPtr<u8>)buffer
                          maxLength:(NSUInteger)buffer_size {
    let &NSURLHostObject::FileURL { ns_string, .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    msg![env; ns_string getFileSystemRepresentation:buffer maxLength:buffer_size]
}

- (id)URLByAppendingPathComponent:(id)path_component // NSString *
                      isDirectory:(bool)is_directory {
    let &NSURLHostObject::FileURL { ns_string, .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    let mut path: id = msg![env; ns_string stringByAppendingPathComponent:path_component];
    if is_directory {
        path = msg![env; path stringByAppendingString:(get_static_str(env, "/"))];
    }
    msg_class![env; NSURL fileURLWithPath:path]
}

- (id)URLByDeletingLastPathComponent {
    let &NSURLHostObject::FileURL { ns_string, .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    let path: id = msg![env; ns_string stringByDeletingLastPathComponent];
    msg_class![env; NSURL fileURLWithPath:path]
}

// TODO: more constructors, more accessors

@end

};

/// Shortcut for host code, provides a view of a URL as a path.
/// TODO: Try to avoid allocating a new GuestPathBuf in more cases.
pub fn to_rust_path(env: &mut Environment, url: id) -> Cow<'static, GuestPath> {
    let path_string: id = msg![env; url path];

    match to_rust_string(env, path_string) {
        Cow::Borrowed(path) => Cow::Borrowed(path.as_ref()),
        Cow::Owned(path_buf) => Cow::Owned(path_buf.into()),
    }
}

/// Splits a URL into its scheme, authority, path, query and fragment, as in
/// RFC 3986 appendix B.
#[allow(clippy::type_complexity)]
fn split_url(url: &str) -> (Option<&str>, Option<&str>, &str, Option<&str>, Option<&str>) {
    let (rest, fragment) = match url.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (url, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(query)),
        None => (rest, None),
    };
    let (scheme, rest) = match rest.find(':') {
        Some(i) if i > 0 && !rest[..i].contains('/') => (Some(&rest[..i]), &rest[i + 1..]),
        _ => (None, rest),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(rest) => {
            let i = rest.find('/').unwrap_or(rest.len());
            (Some(&rest[..i]), &rest[i..])
        }
        None => (None, rest),
    };
    (scheme, authority, path, query, fragment)
}

/// Removes `.` and `..` segments from a path (RFC 3986 section 5.2.4).
fn remove_dot_segments(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    let last = segments.len() - 1;
    let mut output: Vec<&str> = Vec::new();
    for (i, &segment) in segments.iter().enumerate() {
        if segment == "." || segment == ".." {
            // Never remove the empty segment before a leading slash.
            if segment == ".."
                && output
                    .last()
                    .is_some_and(|s| !s.is_empty() || output.len() > 1)
            {
                output.pop();
            }
            if i == last {
                output.push("");
            }
        } else {
            output.push(segment);
        }
    }
    output.join("/")
}

/// Resolves a URL reference against a base URL (RFC 3986 section 5.2).
fn resolve_url_reference(base: &str, reference: &str) -> String {
    let (b_scheme, b_authority, b_path, b_query, _) = split_url(base);
    let (r_scheme, r_authority, r_path, r_query, fragment) = split_url(reference);
    let (scheme, authority, path, query);
    if r_scheme.is_some() {
        (scheme, authority) = (r_scheme, r_authority);
        (path, query) = (remove_dot_segments(r_path), r_query);
    } else if r_authority.is_some() {
        (scheme, authority) = (b_scheme, r_authority);
        (path, query) = (remove_dot_segments(r_path), r_query);
    } else if r_path.is_empty() {
        (scheme, authority) = (b_scheme, b_authority);
        (path, query) = (b_path.to_string(), r_query.or(b_query));
    } else if r_path.starts_with('/') {
        (scheme, authority) = (b_scheme, b_authority);
        (path, query) = (remove_dot_segments(r_path), r_query);
    } else {
        let merged = if b_authority.is_some() && b_path.is_empty() {
            format!("/{}", r_path)
        } else {
            let dir_end = b_path.rfind('/').map_or(0, |i| i + 1);
            format!("{}{}", &b_path[..dir_end], r_path)
        };
        (scheme, authority) = (b_scheme, b_authority);
        (path, query) = (remove_dot_segments(&merged), r_query);
    }

    let mut result = String::new();
    if let Some(scheme) = scheme {
        result.push_str(scheme);
        result.push(':');
    }
    if let Some(authority) = authority {
        result.push_str("//");
        result.push_str(authority);
    }
    result.push_str(&path);
    if let Some(query) = query {
        result.push('?');
        result.push_str(query);
    }
    if let Some(fragment) = fragment {
        result.push('#');
        result.push_str(fragment);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::resolve_url_reference;

    #[test]
    fn test_resolve_url_reference() {
        // The examples from RFC 3986 section 5.4.
        let base = "http://a/b/c/d;p?q";
        let examples = [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("g;x", "http://a/b/c/g;x"),
            ("g;x?y#s", "http://a/b/c/g;x?y#s"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
            ("../../../g", "http://a/g"),
            ("../../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
            ("g?y/./x", "http://a/b/c/g?y/./x"),
            ("g#s/../x", "http://a/b/c/g#s/../x"),
        ];
        for (reference, expected) in examples {
            assert_eq!(
                resolve_url_reference(base, reference),
                expected,
                "{reference}"
            );
        }
    }
}
