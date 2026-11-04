/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSObject`, the root of most class hierarchies in Objective-C.
//!
//! Resources:
//! - Apple's [Advanced Memory Management Programming Guide](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/MemoryMgmt/Articles/MemoryMgmt.html)
//!   explains how reference counting works. Note that we are interested in what
//!   it calls "manual retain-release", not ARC.
//! - Apple's [Key-Value Coding Programming Guide](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/KeyValueCoding/SearchImplementation.html)
//!   explains the algorithms `setValue:forKey:` and `valueForKey:` should
//!   follow.
//!
//! See also: [crate::objc], especially the `objects` module.

use super::ns_string::{from_rust_string, to_rust_string};
use super::{ns_key_value_observing, NSTimeInterval, NSUInteger};
use crate::abi::GuestFunction;
use crate::frameworks::foundation::ns_run_loop::{
    add_perform_request, cancel_perform_requests, run_loop_for_thread_id,
};
use crate::frameworks::foundation::ns_thread::{detach_new_thread_inner, thread_id_for_ns_thread};
use crate::libc::semaphore::{host_destroy_semaphore, sem_wait};
use crate::mem::MutVoidPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, msg_send, msg_send_no_type_checking, nil, objc_classes,
    release, retain, Class, ClassExports, NSZonePtr, ObjC, TrivialHostObject, SEL, TYPE_BOOL,
    TYPE_CHAR, TYPE_CHAR_PTR, TYPE_CLASS, TYPE_DOUBLE, TYPE_FLOAT, TYPE_ID, TYPE_INT, TYPE_LONG,
    TYPE_LONGLONG, TYPE_SEL, TYPE_SHORT, TYPE_UCHAR, TYPE_UINT, TYPE_ULONG, TYPE_ULONGLONG,
    TYPE_UNDEF, TYPE_USHORT, TYPE_VOID,
};
use crate::Environment;

/// The `NSMethodSignature*` of an instance method of `class`, or nil if
/// there's no such method.
/// Performs a selector straight away, for `waitUntilDone:` on the thread
/// it's for.
fn perform_now(env: &mut Environment, this: id, sel: SEL, arg: id) {
    if sel.as_str(&env.mem).ends_with(':') {
        // Like performSelector:withObject:, the method may take a
        // non-object argument.
        () = msg_send_no_type_checking(env, (this, sel, arg));
    } else {
        // As on a device, a method without one ignores the argument.
        () = msg_send(env, (this, sel));
    }
}

fn method_signature(env: &mut Environment, class: Class, sel: SEL) -> id {
    if !env.objc.class_has_method(class, sel) {
        return nil;
    }
    // TODO: support `host` method signatures
    let Some(&sig) = env.objc.class_get_method_signature(class, sel) else {
        panic!(
            "TODO: method signature of host method {}",
            sel.as_str(&env.mem)
        );
    };
    log_dbg!(
        "method signature: '{}' -> {:?}",
        sel.as_str(&env.mem),
        env.mem.cstr_at_utf8(sig)
    );
    msg_class![env; NSMethodSignature signatureWithObjCTypes:sig]
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSObject

+ (id)alloc {
    msg![env; this allocWithZone:(MutVoidPtr::null())]
}
+ (id)allocWithZone:(NSZonePtr)_zone { // struct _NSZone*
    log_dbg!("[{:?} allocWithZone:]", this);
    env.objc.alloc_object(this, Box::new(TrivialHostObject), &mut env.mem)
}

+ (id)new {
    let new_object: id = msg![env; this alloc];
    msg![env; new_object init]
}

+ (Class)class {
    this
}
+ (id)self {
    this
}
+ (Class)superclass {
    env.objc.get_superclass(this)
}
+ (bool)isSubclassOfClass:(Class)class {
    env.objc.class_is_subclass_of(this, class)
}

// See the instance method section for the normal versions of these.
+ (id)retain {
    this // classes are not refcounted
}
+ (())release {
    // classes are not refcounted
}
+ (())autorelease {
    // classes are not refcounted
}

+ (bool)instancesRespondToSelector:(SEL)selector {
    env.objc.class_has_method(this, selector)
}

+ (())cancelPreviousPerformRequestsWithTarget:(id)target {
    let run_loop: id = msg_class![env; NSRunLoop currentRunLoop];
    cancel_perform_requests(env, run_loop, target, None, nil);
}

+ (())cancelPreviousPerformRequestsWithTarget:(id)target selector:(SEL)selector object:(id)arg {
    let run_loop: id = msg_class![env; NSRunLoop currentRunLoop];
    cancel_perform_requests(env, run_loop, target, Some(selector), arg);
}

+ (bool)accessInstanceVariablesDirectly {
    true
}

+ (id)description {
    let name = env.objc.get_class_name(this);
    let str = from_rust_string(env, name.to_string());
    autorelease(env, str)
}

+ (id)debugDescription {
    msg![env; this description]
}

+ (id)instanceMethodSignatureForSelector:(SEL)sel {
    method_signature(env, this, sel)
}

+ (GuestFunction)instanceMethodForSelector:(SEL)selector {
    ObjC::class_get_method_guest_function(env, this, selector)
}

+ (())initialize {
    // Do nothing
}

- (id)init {
    this
}

- (NSUInteger)retainCount {
    env.objc.get_refcount(this).into()
}

- (id)retain {
    log_dbg!("[{:?} retain]", this);
    env.objc.increment_refcount(this);
    this
}
- (())release {
    log_dbg!("[{:?} release]", this);
    if env.objc.decrement_refcount(this) {
        () = msg![env; this dealloc];
    }
}
- (id)autorelease {
    () = msg_class![env; NSAutoreleasePool addObject:this];
    this
}

- (())dealloc {
    log_dbg!("[{:?} dealloc]", this);
    ns_key_value_observing::forget_object(env, this);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)self {
    this
}
- (Class)class {
    ObjC::read_isa(this, &env.mem)
}
- (Class)superclass {
    let class = ObjC::read_isa(this, &env.mem);
    env.objc.get_superclass(class)
}
- (bool)isMemberOfClass:(Class)class {
    let this_class: Class = msg![env; this class];
    class == this_class
}
- (bool)isKindOfClass:(Class)class {
    let this_class: Class = msg![env; this class];
    env.objc.class_is_subclass_of(this_class, class)
}

- (NSUInteger)hash {
    this.to_bits()
}

// To not confuse with isEqualTo:, which is
// a category of NSWhoseSpecifier!
// Reference https://nshipster.com/equality
- (bool)isEqual:(id)other {
    this == other
}

// TODO: Instance description and debugDescription.
// This is not hard to add, but before adding a fallback implementation of it,
// we should make sure all the Foundation classes' overrides of it are there,
// to prevent weird behavior.
// TODO: localized description methods also? (not sure if NSObject has them)

// Helper for NSCopying
- (id)copy {
    msg![env; this copyWithZone:(MutVoidPtr::null())]
}

// Helper for NSMutableCopying
- (id)mutableCopy {
    msg![env; this mutableCopyWithZone:(MutVoidPtr::null())]
}

// NSKeyValueCoding
// https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/KeyValueCoding/SearchImplementation.html
- (())setValue:(id)value
       forKey:(id)key { // NSString*
    let key_string = to_rust_string(env, key); // TODO: avoid copy?
    assert!(key_string.is_ascii()); // TODO: do we have to handle non-ASCII keys?
    let camel_case_key_string = format!("{}{}", key_string.as_bytes()[0].to_ascii_uppercase() as char, &key_string[1..]);

    let class = msg![env; this class];

    // TODO: If value is nil, the target ivar/method argument type must be
    // checked. If it's non-object type, invoke setNilValueForKey:
    // Object properties and setValue:forUndefinedKey: take nil as it is.
    let is_ns_value = value != nil && {
        let value_class = msg![env; value class];
        let ns_value_class = env.objc.get_known_class("NSValue", &mut env.mem);
        env.objc.class_is_subclass_of(value_class, ns_value_class)
    };

    // Look for the first accessor named set<Key>: or _set<Key>, in that order.
    // If found, invoke it with the input value (or unwrapped value, as needed)
    // and finish.
    if let Some(sel) = env.objc.lookup_selector(&format!("set{camel_case_key_string}:")).filter(|&sel| env.objc.class_has_method(class, sel))
        .or_else(|| env.objc.lookup_selector(&format!("_set{camel_case_key_string}:"))).filter(|&sel| env.objc.class_has_method(class, sel)
    ) {
        if is_ns_value {
            // If value is a NSValue, it must be unwrapped
            // Find the selector's first (and only) argument type:
            let types = env.objc.get_class_method(class, sel).types.clone();
            let mut type_index = 0;
            let mut argument_type = None;
            assert_ne!(types.len(), 0);
            for character in types.chars() {
                // TODO: Handle arrays, structs, unions, bitfields, pointers
                // and other types that span multiple chars
                if matches!(character, TYPE_ID | TYPE_CLASS | TYPE_SEL | TYPE_CHAR | TYPE_UCHAR | TYPE_SHORT | TYPE_USHORT | TYPE_INT | TYPE_UINT | TYPE_LONG | TYPE_ULONG | TYPE_LONGLONG | TYPE_ULONGLONG | TYPE_FLOAT | TYPE_DOUBLE | TYPE_BOOL | TYPE_VOID | TYPE_CHAR_PTR | TYPE_UNDEF) {
                    match type_index {
                        // First type is the return type
                        0 => assert_eq!(character, TYPE_VOID),
                        // Second type in methods must be the selector
                        1 => assert_eq!(character, TYPE_SEL),
                        // Third type is the method's first argument type
                        2 => argument_type = Some(character),
                        // Panic if there's more than one type
                        _ => panic!(),
                    }
                    type_index += 1;
                }
            }

            match argument_type.unwrap() {
                TYPE_BOOL => {
                    let value: bool = msg![env; value boolValue];
                    () = msg_send(env, (this, sel, value));
                },
                TYPE_DOUBLE => {
                    let value: f64 = msg![env; value doubleValue];
                    () = msg_send(env, (this, sel, value));
                },
                TYPE_FLOAT => {
                    let value: f32 = msg![env; value floatValue];
                    () = msg_send(env, (this, sel, value));
                },
                TYPE_INT => {
                    let value: i32 = msg![env; value intValue];
                    () = msg_send(env, (this, sel, value));
                },
                TYPE_LONGLONG => {
                    let value: i64 = msg![env; value longLongValue];
                    () = msg_send(env, (this, sel, value));
                },
                TYPE_UINT => {
                    let value: u32 = msg![env; value unsignedIntValue];
                    () = msg_send(env, (this, sel, value));
                },
                TYPE_ULONGLONG => {
                    let value: u64 = msg![env; value unsignedLongLongValue];
                    () = msg_send(env, (this, sel, value));
                },
                argument_type => unimplemented!("TODO: [(NSObject*){:?} setValue:{:?} forKey:{:?}]: Received an NSValue which must be unwrapped to type {}", this, value, key, argument_type)
            };
        } else {
            () = msg_send(env, (this, sel, value));
        }
        return;
    }

    // If no simple accessor is found, and if the class method
    // accessInstanceVariablesDirectly returns YES, look for an instance
    // variable with a name like _<key>, _is<Key>, <key>, or is<Key>,
    // in that order.
    // If found, set the variable directly with the input value
    // (or unwrapped value) and finish.
    let sel = env.objc.lookup_selector("accessInstanceVariablesDirectly").unwrap();
    let accessInstanceVariablesDirectly = msg_send(env, (class, sel));
    if accessInstanceVariablesDirectly {
        if let Some((ivar_ptr, ivar_type)) = env.objc.object_lookup_ivar(&env.mem, this, &format!("_{key_string}"))
            .or_else(|| env.objc.object_lookup_ivar(&env.mem, this, &format!("_is{camel_case_key_string}")))
            .or_else(|| env.objc.object_lookup_ivar(&env.mem, this, &format!("{key_string}")))
            .or_else(|| env.objc.object_lookup_ivar(&env.mem, this, &format!("is{camel_case_key_string}"))
        ) {
            if is_ns_value {
                // If value is a NSValue, it must be unwrapped
                match ivar_type.chars().next().unwrap() {
                    TYPE_BOOL => {
                        let value: bool = msg![env; value boolValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    TYPE_DOUBLE => {
                        let value: f64 = msg![env; value doubleValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    TYPE_FLOAT => {
                        let value: f32 = msg![env; value floatValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    TYPE_INT => {
                        let value: i32 = msg![env; value intValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    TYPE_LONGLONG => {
                        let value: i64 = msg![env; value longLongValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    TYPE_UINT => {
                        let value: u32 = msg![env; value unsignedIntValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    TYPE_ULONGLONG => {
                        let value: u64 = msg![env; value unsignedLongLongValue];
                        env.mem.write(ivar_ptr.cast(), value);
                    },
                    _ => unimplemented!("TODO: [(NSObject*){:?} setValue:{:?} forKey:{:?}]: Received an NSValue which must be unwrapped to type {}", this, value, key, ivar_type)
                };
            } else {
                retain(env, value);
                env.mem.write(ivar_ptr.cast(), value);
            };
            return;
        }
    }

    // Upon finding no accessor or instance variable,
    // invoke setValue:forUndefinedKey:.
    // This raises an exception by default, but a subclass of NSObject
    // may provide key-specific behavior.
    let sel = env.objc.lookup_selector("setValue:forUndefinedKey:").unwrap();
    () = msg_send(env, (this, sel, value, key));
}

- (id)valueForKey:(id)key { // NSString*
    let key_string = to_rust_string(env, key); // TODO: avoid copy?
    assert!(key_string.is_ascii()); // TODO: do we have to handle non-ASCII keys?
    let camel_case_key_string = format!("{}{}", key_string.as_bytes()[0].to_ascii_uppercase() as char, &key_string[1..]);

    let class = msg![env; this class];

    // Look for the first accessor named get<Key>, <key>, is<Key>, or _<key>,
    // in that order. If found, invoke it, wrapping a non-object result.
    // TODO: collection accessors (countOf<Key> and friends)
    for name in [
        format!("get{camel_case_key_string}"),
        key_string.to_string(),
        format!("is{camel_case_key_string}"),
        format!("_{key_string}"),
    ] {
        let Some(sel) = env.objc.lookup_selector(&name) else {
            continue;
        };
        if !env.objc.class_has_method(class, sel) {
            continue;
        }
        let types = env.objc.get_class_method(class, sel).types.clone();
        let return_type = types.trim_start_matches(['r', 'n', 'N', 'o', 'O', 'R', 'V']).chars().next().unwrap();
        return match return_type {
            TYPE_ID | TYPE_CLASS => msg_send_no_type_checking(env, (this, sel)),
            TYPE_CHAR => {
                let value: i8 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithChar:value]
            }
            TYPE_BOOL => {
                let value: bool = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithBool:value]
            }
            TYPE_SHORT => {
                let value: i16 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithShort:value]
            }
            TYPE_USHORT => {
                let value: u16 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithUnsignedShort:value]
            }
            TYPE_INT | TYPE_LONG => {
                let value: i32 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithInt:value]
            }
            TYPE_UINT | TYPE_ULONG => {
                let value: u32 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithUnsignedInt:value]
            }
            TYPE_LONGLONG => {
                let value: i64 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithLongLong:value]
            }
            TYPE_ULONGLONG => {
                let value: u64 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithUnsignedLongLong:value]
            }
            TYPE_FLOAT => {
                let value: f32 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithFloat:value]
            }
            TYPE_DOUBLE => {
                let value: f64 = msg_send_no_type_checking(env, (this, sel));
                msg_class![env; NSNumber numberWithDouble:value]
            }
            _ => unimplemented!("TODO: [(NSObject*){:?} valueForKey:{:?}]: wrapping a value of type {}", this, key_string, types),
        };
    }

    // If no simple accessor is found, and if the class method
    // accessInstanceVariablesDirectly returns YES, look for an instance
    // variable with a name like _<key>, _is<Key>, <key>, or is<Key>,
    // in that order. If found, get its value, wrapping a non-object one.
    let sel = env.objc.lookup_selector("accessInstanceVariablesDirectly").unwrap();
    let accessInstanceVariablesDirectly = msg_send(env, (class, sel));
    if accessInstanceVariablesDirectly {
        if let Some((ivar_ptr, ivar_type)) = env.objc.object_lookup_ivar(&env.mem, this, &format!("_{key_string}"))
            .or_else(|| env.objc.object_lookup_ivar(&env.mem, this, &format!("_is{camel_case_key_string}")))
            .or_else(|| env.objc.object_lookup_ivar(&env.mem, this, &key_string.to_string()))
            .or_else(|| env.objc.object_lookup_ivar(&env.mem, this, &format!("is{camel_case_key_string}"))
        ) {
            let ivar_type = ivar_type.to_string();
            let ptr = ivar_ptr.cast_const();
            return match ivar_type.chars().next().unwrap() {
                TYPE_ID | TYPE_CLASS => env.mem.read(ptr.cast()),
                TYPE_CHAR => {
                    let value: i8 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithChar:value]
                }
                TYPE_BOOL => {
                    let value: bool = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithBool:value]
                }
                TYPE_SHORT => {
                    let value: i16 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithShort:value]
                }
                TYPE_USHORT => {
                    let value: u16 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithUnsignedShort:value]
                }
                TYPE_INT | TYPE_LONG => {
                    let value: i32 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithInt:value]
                }
                TYPE_UINT | TYPE_ULONG => {
                    let value: u32 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithUnsignedInt:value]
                }
                TYPE_LONGLONG => {
                    let value: i64 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithLongLong:value]
                }
                TYPE_ULONGLONG => {
                    let value: u64 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithUnsignedLongLong:value]
                }
                TYPE_FLOAT => {
                    let value: f32 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithFloat:value]
                }
                TYPE_DOUBLE => {
                    let value: f64 = env.mem.read(ptr.cast());
                    msg_class![env; NSNumber numberWithDouble:value]
                }
                _ => unimplemented!("TODO: [(NSObject*){:?} valueForKey:{:?}]: wrapping an ivar of type {}", this, key_string, ivar_type),
            };
        }
    }

    // Upon finding no accessor or instance variable,
    // invoke valueForUndefinedKey:.
    // This raises an exception by default, but a subclass of NSObject
    // may provide key-specific behavior.
    let sel = env.objc.lookup_selector("valueForUndefinedKey:").unwrap();
    msg_send(env, (this, sel, key))
}

- (id)valueForUndefinedKey:(id)key { // NSString*
    // TODO: Raise NSUnknownKeyException
    let class: Class = ObjC::read_isa(this, &env.mem);
    let class_name_string = env.objc.get_class_name(class).to_owned(); // TODO: Avoid copying
    let key_string = to_rust_string(env, key);
    panic!("Object {:?} of class {:?} ({:?}) does not have a getter for {} ({:?})\
        \nAvailable selectors: {}\nAvailable ivars: {}",
        this, class_name_string, class, key_string, key,
        env.objc.debug_all_class_selectors_as_strings(&env.mem, class).join(", "),
        env.objc.debug_all_class_ivars_as_strings(class).join(", "));
}

- (id)dictionaryWithValuesForKeys:(id)keys { // NSArray*
    let dict: id = msg_class![env; NSMutableDictionary new];
    let count: NSUInteger = msg![env; keys count];
    for i in 0..count {
        let key: id = msg![env; keys objectAtIndex:i];
        let value: id = msg![env; this valueForKey:key];
        // Missing values are NSNull, since a dictionary can't hold nil.
        let value = if value == nil { msg_class![env; NSNull null] } else { value };
        () = msg![env; dict setObject:value forKey:key];
    }
    let res: id = msg![env; dict copy];
    release(env, dict);
    autorelease(env, res)
}

- (())setValue:(id)_value
forUndefinedKey:(id)key { // NSString*
    // TODO: Raise NSUnknownKeyException
    let class: Class = ObjC::read_isa(this, &env.mem);
    let class_name_string = env.objc.get_class_name(class).to_owned(); // TODO: Avoid copying
    let key_string = to_rust_string(env, key);
    panic!("Object {:?} of class {:?} ({:?}) does not have a setter for {} ({:?})\
        \nAvailable selectors: {}\nAvailable ivars: {}",
        this, class_name_string, class, key_string, key,
        env.objc.debug_all_class_selectors_as_strings(&env.mem, class).join(", "),
        env.objc.debug_all_class_ivars_as_strings(class).join(", "));
}

// NSKeyValueObserving
- (())addObserver:(id)observer
       forKeyPath:(id)key_path // NSString *
          options:(NSUInteger)options
          context:(MutVoidPtr)context {
    ns_key_value_observing::add_observer(env, this, observer, key_path, options, context);
}
- (())removeObserver:(id)observer
          forKeyPath:(id)key_path { // NSString *
    ns_key_value_observing::remove_observer(env, this, observer, key_path, None);
}
- (())removeObserver:(id)observer
          forKeyPath:(id)key_path // NSString *
             context:(MutVoidPtr)context {
    ns_key_value_observing::remove_observer(env, this, observer, key_path, Some(context));
}
- (())willChangeValueForKey:(id)key { // NSString *
    ns_key_value_observing::will_change(env, this, key);
}
- (())didChangeValueForKey:(id)key { // NSString *
    ns_key_value_observing::did_change(env, this, key);
}

- (id)methodSignatureForSelector:(SEL)sel {
    let class = ObjC::read_isa(this, &env.mem);
    method_signature(env, class, sel)
}

- (bool)respondsToSelector:(SEL)selector {
    env.objc.object_has_method(&env.mem, this, selector)
}

- (id)performSelector:(SEL)sel {
    assert!(!sel.is_null());
    msg_send_no_type_checking(env, (this, sel))
}

- (id)performSelector:(SEL)sel
           withObject:(id)o1 {
    assert!(!sel.is_null());
    msg_send_no_type_checking(env, (this, sel, o1))
}

- (id)performSelector:(SEL)sel
           withObject:(id)o1
           withObject:(id)o2 {
    assert!(!sel.is_null());
    msg_send_no_type_checking(env, (this, sel, o1, o2))
}

- (())performSelectorInBackground:(SEL)sel
                       withObject:(id)arg {
    detach_new_thread_inner(env, sel, this, arg, /* tolerate_type_mismatch: */ true)
}

- (())performSelector:(SEL)sel withObject:(id)arg afterDelay:(NSTimeInterval)delay {
    let run_loop: id = msg_class![env; NSRunLoop currentRunLoop];
    add_perform_request(env, run_loop, this, sel, arg, Some(delay), false);
}

- (())performSelectorOnMainThread:(SEL)sel withObject:(id)arg waitUntilDone:(bool)wait {
    log_dbg!("performSelectorOnMainThread:{} withObject:{:?} waitUntilDone:{}", sel.as_str(&env.mem), arg, wait);
    if wait && env.current_thread == 0 {
        perform_now(env, this, sel, arg);
        return;
    }
    if env.bundle.bundle_identifier().starts_with("com.gameloft.POP") && (sel == env.objc.lookup_selector("startMovie:").unwrap() || sel == env.objc.lookup_selector("stopMovie").unwrap()) && wait {
        log!("Applying game-specific hack for PoP: WW: ignoring performSelectorOnMainThread:SEL({}) waitUntilDone:true", sel.as_str(&env.mem));
        return;
    }
    if env.bundle.bundle_identifier().starts_with("com.gameloft.Asphalt5") && (sel == env.objc.lookup_selector("startMovie:").unwrap() || sel == env.objc.lookup_selector("stopMovie:").unwrap()) && wait {
        log!("Applying game-specific hack for Asphalt5: ignoring performSelectorOnMainThread:SEL({}) waitUntilDone:true", sel.as_str(&env.mem));
        return;
    }
    if env.bundle.bundle_identifier().starts_with("com.gameloft.SplinterCell") && sel == env.objc.lookup_selector("startMovie:").unwrap() && wait {
        log!("Applying game-specific hack for SplinterCell: ignoring performSelectorOnMainThread:SEL({}) waitUntilDone:true", sel.as_str(&env.mem));
        return;
    }
    if env.bundle.bundle_identifier().starts_with("com.gameloft.AssassinsCreed") && sel == env.objc.lookup_selector("moviePlayerInit:").unwrap() && wait {
        log!("Applying game-specific hack for AssassinsCreed: ignoring performSelectorOnMainThread:SEL(moviePlayerInit:) waitUntilDone:true");
        return;
    }
    if env.bundle.bundle_identifier().starts_with("com.gameloft.Ferrari") && wait {
        if sel == env.objc.lookup_selector("startMovie:").unwrap() {
            log!("Applying game-specific hack for Ferrari GT: ignoring performSelectorOnMainThread:SEL({}) waitUntilDone:true", sel.as_str(&env.mem));
            return;
        }
        if sel == env.objc.lookup_selector("initTextInput:").unwrap() || sel == env.objc.lookup_selector("removeTextField:").unwrap() {
            log!("Applying game-specific hack for Ferrari GT: performing performSelectorOnMainThread:SEL({}) waitUntilDone:true on thread {}", sel.as_str(&env.mem), env.current_thread);
            () = msg_send(env, (this, sel, arg));
            return;
        }
    }
    if env.bundle.bundle_identifier().starts_with("com.gameloft.HOS2") && wait {
        if sel == env.objc.lookup_selector("loadMovie:").unwrap() || sel == env.objc.lookup_selector("sendGameInfo").unwrap() || sel == env.objc.lookup_selector("setStatusBar:").unwrap() {
            log!("Applying game-specific hack for HOS2: performing performSelectorOnMainThread:SEL({}) waitUntilDone:true on thread {}", sel.as_str(&env.mem), env.current_thread);
            if sel.as_str(&env.mem).ends_with(':') {
                () = msg_send(env, (this, sel, arg));
            } else {
                assert!(arg.is_null());
                () = msg_send(env, (this, sel));
            }
            return;
        }
        if sel == env.objc.lookup_selector("startMovie:").unwrap() || sel == env.objc.lookup_selector("stopMovie:").unwrap() {
            log!("Applying game-specific hack for HOS2: ignoring performSelectorOnMainThread:SEL({}) waitUntilDone:true", sel.as_str(&env.mem));
            return;
        }
    }

    let run_loop: id = msg_class![env; NSRunLoop mainRunLoop];
    let sem = add_perform_request(env, run_loop, this, sel, arg, None, wait);
    if wait {
        sem_wait(env, sem);
        host_destroy_semaphore(env, sem);
    }
}

- (())performSelector:(SEL)sel
             onThread:(id)thread // NSThread*
           withObject:(id)arg
        waitUntilDone:(bool)wait {
    let Some(thread_id) = thread_id_for_ns_thread(env, thread) else {
        panic!("performSelector:onThread: for a thread that isn't running");
    };
    if wait && thread_id == env.current_thread {
        perform_now(env, this, sel, arg);
        return;
    }
    let run_loop = run_loop_for_thread_id(env, thread_id);
    let sem = add_perform_request(env, run_loop, this, sel, arg, None, wait);
    if wait {
        sem_wait(env, sem);
        host_destroy_semaphore(env, sem);
    }
}

- (GuestFunction)methodForSelector:(SEL)selector {
    let isa = ObjC::read_isa(this, &env.mem);
    ObjC::class_get_method_guest_function(env, isa, selector)
}

// UINibLoadingAdditions protocol
- (())awakeFromNib {
    // no-op
}

@end

};
