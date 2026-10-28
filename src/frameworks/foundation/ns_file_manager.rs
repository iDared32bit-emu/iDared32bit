/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSFileManager` etc.

use super::ns_enumerator::{fast_enumeration_helper, NSFastEnumerationState};
use super::{ns_array, ns_string, NSUInteger};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::frameworks::foundation::ns_error::{NSCocoaErrorDomain, NSFileReadNoSuchFileError};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::fs::{FsError, FsNodeType, GuestPath, GuestPathBuf};
use crate::mem::{ConstPtr, MutPtr, Ptr};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, ClassExports, HostObject,
};
use crate::Environment;

type NSSearchPathDirectory = NSUInteger;
const NSApplicationDirectory: NSSearchPathDirectory = 1;
const NSLibraryDirectory: NSSearchPathDirectory = 5;
const NSDocumentDirectory: NSSearchPathDirectory = 9;
const NSCachesDirectory: NSSearchPathDirectory = 13;

type NSSearchPathDomainMask = NSUInteger;
const NSUserDomainMask: NSSearchPathDomainMask = 1;

pub const NSFileModificationDate: &str = "NSFileModificationDate";
pub const NSFileSize: &str = "NSFileSize";
const NSFileSystemFreeSize: &str = "NSFileSystemFreeSize";
pub const NSFileType: &str = "NSFileType";
pub const NSFileTypeDirectory: &str = "NSFileTypeDirectory";
pub const NSFileTypeRegular: &str = "NSFileTypeRegular";

pub const CONSTANTS: ConstantExports = &[
    (
        "_NSFileModificationDate",
        HostConstant::NSString(NSFileModificationDate),
    ),
    ("_NSFileSize", HostConstant::NSString(NSFileSize)),
    (
        "_NSFileSystemFreeSize",
        HostConstant::NSString(NSFileSystemFreeSize),
    ),
    ("_NSFileType", HostConstant::NSString(NSFileType)),
    (
        "_NSFileTypeDirectory",
        HostConstant::NSString(NSFileTypeDirectory),
    ),
    (
        "_NSFileTypeRegular",
        HostConstant::NSString(NSFileTypeRegular),
    ),
];

fn NSSearchPathForDirectoriesInDomains(
    env: &mut Environment,
    directory: NSSearchPathDirectory,
    domain_mask: NSSearchPathDomainMask,
    expand_tilde: bool,
) -> id {
    // TODO: other cases not implemented
    assert!(domain_mask == NSUserDomainMask);
    assert!(expand_tilde);

    let dir = match directory {
        NSApplicationDirectory => {
            // This might not actually be correct. I haven't bothered to
            // test it because I can't think of a good reason an iPhone OS app
            // would have to request this;
            // Wolfenstein 3D requests it but never uses it.
            GuestPath::new(crate::fs::APPLICATIONS).to_owned()
        }
        NSDocumentDirectory => env.fs.home_directory().join("Documents"),
        NSLibraryDirectory => env.fs.home_directory().join("Library"),
        NSCachesDirectory => env.fs.home_directory().join("Library").join("Caches"),
        _ => todo!("NSSearchPathDirectory {}", directory),
    };
    let dir = ns_string::from_rust_string(env, String::from(dir));
    let dir_list = ns_array::from_vec(env, vec![dir]);
    autorelease(env, dir_list)
}

fn NSHomeDirectory(env: &mut Environment) -> id {
    let dir = env.fs.home_directory();
    let dir = ns_string::from_rust_string(env, String::from(dir.as_str()));
    autorelease(env, dir)
}

/// Check [crate::fs::Fs::new] for more info for
/// how temporary folder is setup on startup
fn NSTemporaryDirectory(env: &mut Environment) -> id {
    let dir = env.fs.home_directory().join("tmp");
    let dir = ns_string::from_rust_string(env, String::from(dir.as_str()));
    autorelease(env, dir)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(NSHomeDirectory()),
    export_c_func!(NSTemporaryDirectory()),
    export_c_func!(NSSearchPathForDirectoriesInDomains(_, _, _)),
];

#[derive(Default)]
pub struct State {
    default_manager: Option<id>,
}

type NSDirectoryEnumerationOptions = NSUInteger;
const NSDirectoryEnumerationSkipsSubdirectoryDescendants: NSDirectoryEnumerationOptions = 1;
const NSDirectoryEnumerationSkipsPackageDescendants: NSDirectoryEnumerationOptions = 2;
const NSDirectoryEnumerationSkipsHiddenFiles: NSDirectoryEnumerationOptions = 4;

/// A directory being walked: its path relative to the enumerator's base, its
/// entries (name, is it a directory) and the index of the next one.
type WalkedDir = (String, Vec<(String, bool)>, usize);

/// Walks a directory lazily, in name order, so `skipDescendents` can work.
struct NSDirectoryEnumeratorHostObject {
    base: GuestPathBuf,
    stack: Vec<WalkedDir>,
    /// The directory last returned, whose contents come next unless skipped.
    pending_dir: Option<String>,
    /// The path last returned, relative to `base`.
    last: Option<String>,
    options: NSDirectoryEnumerationOptions,
    /// Return absolute `NSURL`s rather than relative paths.
    as_urls: bool,
}
impl HostObject for NSDirectoryEnumeratorHostObject {}

/// The entries of a directory, sorted by name as iPhone OS returns them.
fn list_dir(
    env: &Environment,
    base: &GuestPath,
    rel: &str,
    options: NSDirectoryEnumerationOptions,
) -> Vec<(String, bool)> {
    let dir = if rel.is_empty() {
        base.to_owned()
    } else {
        base.join(rel)
    };
    let Ok(entries) = env.fs.enumerate_with_types(&dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = entries
        .filter(|(name, _)| {
            options & NSDirectoryEnumerationSkipsHiddenFiles == 0 || !name.starts_with('.')
        })
        .map(|(name, kind)| (name.to_string(), matches!(kind, FsNodeType::Directory)))
        .collect();
    entries.sort();
    entries
}

/// Directories that iPhone OS shows as a single file (bundles).
fn is_package(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, ext)| ext);
    matches!(ext, Some("app" | "bundle" | "framework" | "plugin"))
}

fn new_directory_enumerator(
    env: &mut Environment,
    base: GuestPathBuf,
    options: NSDirectoryEnumerationOptions,
    as_urls: bool,
) -> id {
    let entries = list_dir(env, &base, "", options);
    let host_object = Box::new(NSDirectoryEnumeratorHostObject {
        base,
        stack: vec![(String::new(), entries, 0)],
        pending_dir: None,
        last: None,
        options,
        as_urls,
    });
    let class = env
        .objc
        .get_known_class("NSDirectoryEnumerator", &mut env.mem);
    let enumerator = env.objc.alloc_object(class, host_object, &mut env.mem);
    autorelease(env, enumerator)
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSFileManager: NSObject

+ (id)defaultManager {
    if let Some(existing) = env.framework_state.foundation.ns_file_manager.default_manager {
        existing
    } else {
        let new: id = msg![env; this new];
        env.framework_state.foundation.ns_file_manager.default_manager = Some(new);
        new
    }
}

- (id)currentDirectoryPath {
    ns_string::from_rust_string(env, env.fs.working_directory().as_str().to_string())
}

- (bool)changeCurrentDirectoryPath:(id)path {
    let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
    let path = GuestPath::new(&path);
    match env.fs.change_working_directory(path) {
        Ok(_) => true,
        Err(()) => false
    }
}

- (bool)fileExistsAtPath:(id)path { // NSString*
    let res_exists = if path == nil {
        false
    } else {
        let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
        // fileExistsAtPath: will return true for directories
        // hence Fs::exists() rather than Fs::is_file() is appropriate.
        env.fs.exists(GuestPath::new(&path))
    };
    log_dbg!("[(NSFileManager*) {:?} fileExistsAtPath:{:?}] => {}", this, path, res_exists);
    res_exists
}

- (bool)fileExistsAtPath:(id)path // NSString*
             isDirectory:(MutPtr<bool>)is_dir {
    let (res_exists, res_is_dir) = if path == nil {
        (false, false)
    } else {
        // TODO: mutualize with fileExistsAtPath:
        let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
        let guest_path = GuestPath::new(&path);
        (env.fs.exists(guest_path), !env.fs.is_file(guest_path))
    };

    if !is_dir.is_null() {
        env.mem.write(is_dir, res_is_dir);
    }

    log_dbg!("[(NSFileManager*) {:?} fileExistsAtPath:{:?} isDirectory:{:?}] => {}", this, path, res_is_dir, res_exists);
    res_exists
}

- (bool)createFileAtPath:(id)path // NSString*
                contents:(id)data // NSData*
              attributes:(id)attributes { // NSDictionary*
    assert!(attributes == nil); // TODO
    if data == nil {
        let empty: id = msg_class![env; NSData new];
        let res: bool = msg![env; empty writeToFile:path atomically:false];
        release(env, empty);
        res
    } else {
        msg![env; data writeToFile:path atomically:false]
    }
}

- (bool)removeItemAtPath:(id)path // NSString*
                   error:(MutPtr<id>)out_error { // NSError**
    // TODO: call delegate
    let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
    match env.fs.remove(GuestPath::new(&path)) {
        Ok(()) => true,
        Err(err) => {
            if !out_error.is_null() {
                match err {
                    FsError::DoesNotExist => {
                        let domain = get_static_str(env, NSCocoaErrorDomain);
                        let error = msg_class![env; NSError alloc];
                        let error = msg![env; error initWithDomain:domain code:NSFileReadNoSuchFileError userInfo:nil];
                        autorelease(env, error);
                        env.mem.write(out_error, error);
                    }
                    _ => unimplemented!()
                }
            }
            false
        }
    }
}

- (bool)moveItemAtPath:(id)path // NSString*
                toPath:(id)toPath // NSString*
                 error:(MutPtr<id>)error { // NSError**
    // TODO: call delegate
    let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
    let toPath = ns_string::to_rust_string(env, toPath); // TODO: avoid copy
    match env.fs.rename(GuestPath::new(&path), GuestPath::new(&toPath)) {
        Ok(()) => true,
        Err(_) => {
            if !error.is_null() {
               todo!(); // TODO: create an NSError if requested
            }
            false
        }
    }
}

- (bool)createDirectoryAtPath:(id)path // NSString *
                   attributes:(id)attributes { // NSDictionary*
    let error: MutPtr<id> = Ptr::null();
    msg![env; this createDirectoryAtPath:path
             withIntermediateDirectories:false
                              attributes:attributes
                                   error:error]
}

- (bool)createDirectoryAtPath:(id)path // NSString *
  withIntermediateDirectories:(bool)with_intermediates
                   attributes:(id)attributes // NSDictionary*
                        error:(MutPtr<id>)error { // NSError**
    assert_eq!(attributes, nil); // TODO

    let path_str = ns_string::to_rust_string(env, path); // TODO: avoid copy
    let res = if with_intermediates {
        env.fs.create_dir_all(GuestPath::new(&path_str))
    } else {
        env.fs.create_dir(GuestPath::new(&path_str))
    };
    match res {
        Ok(()) => {
            log_dbg!("createDirectoryAtPath {} => true", path_str);
            true
        }
        Err(err) => {
            assert!(error.is_null()); // TODO
            log!(
                "Warning: createDirectoryAtPath {} failed with {:?}, returning false",
                path_str,
                err,
            );
            false
        }
    }
}

- (id)enumeratorAtPath:(id)path { // NSString*
    let path = GuestPathBuf::from(ns_string::to_rust_string(env, path).into_owned());
    if !env.fs.is_dir(&path) {
        return nil;
    }
    new_directory_enumerator(env, path, 0, false)
}

- (id)enumeratorAtURL:(id)url // NSURL*
includingPropertiesForKeys:(id)_keys // NSArray*, only prefetching
                   options:(NSDirectoryEnumerationOptions)options
              errorHandler:(id)error_handler { // block
    let path: id = msg![env; url path];
    let path = GuestPathBuf::from(ns_string::to_rust_string(env, path).into_owned());
    // Like on iOS, a URL that isn't a directory gives an enumerator that
    // returns nothing.
    if !env.fs.is_dir(&path) && error_handler != nil {
        log!("TODO: enumeratorAtURL: errorHandler not called for {:?}", path);
    }
    new_directory_enumerator(env, path, options, true)
}

- (id)directoryContentsAtPath:(id)path /* NSString* */ { // NSArray*
    let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
    let Ok(paths) = env.fs.enumerate(GuestPath::new(&path)) else {
        return nil;
    };
    let paths: Vec<GuestPathBuf> = paths
        .map(|path| GuestPathBuf::from(GuestPath::new(path)))
        .collect();
    log_dbg!("directoryContentsAtPath {}: {:?}", path, paths);
    let path_strings = paths
        .iter()
        .map(|name| ns_string::from_rust_string(env, name.as_str().to_string()))
        .collect();
    let res = ns_array::from_vec(env, path_strings);
    autorelease(env, res)
}

- (id)contentsOfDirectoryAtPath:(id)path /* NSString* */
                          error:(MutPtr<id>)error { // NSError**
    let contents: id = msg![env; this directoryContentsAtPath:path];
    if contents == nil && !error.is_null() {
        todo!(); // TODO: create an NSError if requested
    }
    contents
}

- (bool)isReadableFileAtPath:(id)path { // NSString*
    let (_, readable, _, _) = {
        let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
        env.fs.access(GuestPath::new(&path))
    };
    readable
}

- (bool)isWritableFileAtPath:(id)path { // NSString*
    let (_, _, writable, _) = {
        let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
        env.fs.access(GuestPath::new(&path))
    };
    writable
}

- (bool)isDeletableFileAtPath:(id)path { // NSString*
    let is_file = {
        let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
        env.fs.is_file(GuestPath::new(&path))
    };

    if is_file {
        return msg![env; this isWritableFileAtPath:path];
    }

    let directory_enumerator: id = msg![env; this enumeratorAtPath:path];

    let mut is_deletable = true;
    loop {
        let path: id = msg![env; directory_enumerator nextObject];
        if path == nil {
            break;
        }
        let is_path_deletable: bool = msg![env; this isDeletableFileAtPath:path];
        is_deletable &= is_path_deletable;
        if !is_deletable {
            break;
        }
    }
    is_deletable
}

- (id)contentsAtPath:(id)path { // NSString *
    // TODO: return nil if path is directory
    // TODO: handle non-absolute paths?
    assert!(msg![env; path isAbsolutePath]);
    msg_class![env; NSData dataWithContentsOfFile:path]
}

- (bool)copyItemAtPath:(id)src // NSString*
                toPath:(id)dst // NSString*
                 error:(MutPtr<id>)error { // NSError**
    let src = ns_string::to_rust_string(env, src);
    let dst = ns_string::to_rust_string(env, dst);
    let data = match env.fs.read(GuestPath::new(src.as_ref())) {
        Ok(d) => d,
        Err(_) => {
            assert!(error.is_null()); // TODO
            return false;
        }
    };
    if env.fs.write(GuestPath::new(dst.as_ref()), &data).is_err() {
        assert!(error.is_null()); // TODO
        return false;
    }
    true
}

- (ConstPtr<u8>)fileSystemRepresentationWithPath:(id)path { // NSString*
    let length: NSUInteger = msg![env; path length];
    assert!(length > 0);
    // TODO: throw an exception if conversion fails
    msg![env; path UTF8String]
}

- (id)fileAttributesAtPath:(id)path // NSString *
              traverseLink:(bool)traverse {
    // TODO: other attributes
    log_once!("Warning: NSFileManager fileAttributesAtPath:traverseLink: returns only NSFileType, NSFileModificationDate and NSFileSize attributes!");

    let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
    // TODO: traverse link
    log_dbg!("[(NSFileManager *){:?} fileAttributesAtPath:{} traverse:{}]", this, path, traverse);
    let guest_path = GuestPath::new(&path);

    file_attributes_common(env, guest_path)
}

- (id)attributesOfItemAtPath:(id)path // NSString *
                       error:(MutPtr<id>)error { // NSError **
    assert!(error.is_null()); // TODO

    // TODO: other attributes
    log_once!("Warning: NSFileManager attributesOfItemAtPath:error: returns only NSFileType, NSFileModificationDate and NSFileSize attributes!");

    let path = ns_string::to_rust_string(env, path); // TODO: avoid copy
    // TODO: traverse link
    log_dbg!("[(NSFileManager *){:?} attributesOfItemAtPath:{} error:{:?}]", this, path, error);
    let guest_path = GuestPath::new(&path);

    file_attributes_common(env, guest_path)
}

// Deprecated in iPhone OS 2.0.
- (id)fileSystemAttributesAtPath:(id)path { // NSString*
    msg![env; this attributesOfFileSystemForPath:path error:(MutPtr::<id>::null())]
}

- (id)attributesOfFileSystemForPath:(id)_path
                              error:(MutPtr<id>)error {
    // TODO: other attributes
    log_once!("Warning: NSFileManager attributesOfFileSystemForPath:error: returns only NSFileSystemFreeSize attribute!");

    assert!(error.is_null()); // TODO

    let dict = msg_class![env; NSMutableDictionary new];

    // Reporting 1 Gb of free space should be enough
    // TODO: unify with `statfs`
    // TODO: account for path
    let size: u64 = 1024 * 1024 * 1024;
    let size_num: id = msg_class![env; NSNumber numberWithUnsignedLongLong:size];

    let fs_free_size_key = get_static_str(env, NSFileSystemFreeSize);
    () = msg![env; dict setObject:size_num forKey:fs_free_size_key];

    let dict_imm = msg![env; dict copy];
    release(env, dict);
    autorelease(env, dict_imm)
}

@end

@implementation NSDirectoryEnumerator: NSEnumerator

- (id)nextObject {
    let host_obj = env.objc.borrow_mut::<NSDirectoryEnumeratorHostObject>(this);
    if let Some(dir) = host_obj.pending_dir.take() {
        let (base, options) = (host_obj.base.clone(), host_obj.options);
        let entries = list_dir(env, &base, &dir, options);
        let host_obj = env.objc.borrow_mut::<NSDirectoryEnumeratorHostObject>(this);
        host_obj.stack.push((dir, entries, 0));
    }
    let host_obj = env.objc.borrow_mut::<NSDirectoryEnumeratorHostObject>(this);
    let rel = loop {
        let Some((dir, entries, next)) = host_obj.stack.last_mut() else {
            host_obj.last = None;
            return nil;
        };
        let Some((name, is_dir)) = entries.get(*next) else {
            host_obj.stack.pop();
            continue;
        };
        *next += 1;
        let rel = if dir.is_empty() {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        let options = host_obj.options;
        if *is_dir
            && options & NSDirectoryEnumerationSkipsSubdirectoryDescendants == 0
            && (options & NSDirectoryEnumerationSkipsPackageDescendants == 0 || !is_package(name))
        {
            host_obj.pending_dir = Some(rel.clone());
        }
        break rel;
    };
    host_obj.last = Some(rel.clone());
    if host_obj.as_urls {
        let path = host_obj.base.join(&rel);
        let path = ns_string::from_rust_string(env, String::from(path));
        let path = autorelease(env, path);
        msg_class![env; NSURL fileURLWithPath:path]
    } else {
        let rel = ns_string::from_rust_string(env, rel);
        autorelease(env, rel)
    }
}

// One object at a time, as on iOS, so that fileAttributes, level and
// skipDescendents apply to the object the loop is on.
- (NSUInteger)countByEnumeratingWithState:(MutPtr<NSFastEnumerationState>)state
                                  objects:(MutPtr<id>)stackbuf
                                    count:(NSUInteger)len {
    fast_enumeration_helper(env, this, |env, _| {
        msg![env; this nextObject]
    }, state, stackbuf, len.min(1))
}

// Not to be confused with skipDescendants (iOS 4), which does the same.
- (())skipDescendents {
    env.objc.borrow_mut::<NSDirectoryEnumeratorHostObject>(this).pending_dir = None;
}
- (())skipDescendants {
    env.objc.borrow_mut::<NSDirectoryEnumeratorHostObject>(this).pending_dir = None;
}

- (NSUInteger)level {
    let host_obj = env.objc.borrow::<NSDirectoryEnumeratorHostObject>(this);
    host_obj.last.as_ref().map_or(0, |rel| rel.split('/').count() as NSUInteger)
}

- (id)fileAttributes {
    let host_obj = env.objc.borrow::<NSDirectoryEnumeratorHostObject>(this);
    let Some(rel) = &host_obj.last else {
        return nil;
    };
    let path = host_obj.base.join(rel);
    file_attributes_common(env, &path)
}

- (id)directoryAttributes {
    let base = env.objc.borrow::<NSDirectoryEnumeratorHostObject>(this).base.clone();
    file_attributes_common(env, &base)
}

@end

};

/// Helper function for `fileAttributesAtPath:traverseLink:` and
/// `attributesOfItemAtPath:error:`
fn file_attributes_common(env: &mut Environment, guest_path: &GuestPath) -> id {
    if !env.fs.exists(guest_path) {
        log!(
            "file_attributes_common() called with file that does not exist: {:?}, Returning nil",
            guest_path
        );
        return nil;
    }

    // TODO: support more attributes
    let unix_timestamp: f64 = env.fs.modified(guest_path).unwrap() as f64;
    let unix_ref_date: id = msg_class![env; NSDate dateWithTimeIntervalSince1970:0f64];
    let unix_date: id =
        msg_class![env; NSDate dateWithTimeInterval:unix_timestamp sinceDate:unix_ref_date];

    let size = env.fs.size(guest_path).unwrap();
    let size_num: id = msg_class![env; NSNumber numberWithUnsignedLongLong:size];

    let dict = msg_class![env; NSMutableDictionary new];

    let modif_date_key = get_static_str(env, NSFileModificationDate);
    () = msg![env; dict setObject:unix_date forKey:modif_date_key];

    let size_key = get_static_str(env, NSFileSize);
    () = msg![env; dict setObject:size_num forKey:size_key];

    let file_type_key = get_static_str(env, NSFileType);
    // TODO: other types
    if env.fs.is_file(guest_path) {
        let file_type_regular = get_static_str(env, NSFileTypeRegular);
        () = msg![env; dict setObject:file_type_regular forKey:file_type_key];
    } else if env.fs.is_dir(guest_path) {
        let file_type_directory = get_static_str(env, NSFileTypeDirectory);
        () = msg![env; dict setObject:file_type_directory forKey:file_type_key];
    }

    let dict_imm = msg![env; dict copy];
    release(env, dict);
    autorelease(env, dict_imm)
}
