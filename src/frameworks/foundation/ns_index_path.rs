/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSIndexPath`, including UIKit's additions for table views.

use super::ns_string::from_rust_ordering;
use super::{NSComparisonResult, NSInteger, NSNotFound, NSUInteger};
use crate::mem::{ConstPtr, MutPtr};
use crate::objc::{
    autorelease, id, msg, msg_class, objc_classes, retain, Class, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;

#[derive(Default)]
struct NSIndexPathHostObject {
    indexes: Vec<NSUInteger>,
}
impl HostObject for NSIndexPathHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSIndexPath: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSIndexPathHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)indexPathWithIndex:(NSUInteger)index {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithIndex:index];
    autorelease(env, new)
}
+ (id)indexPathWithIndexes:(ConstPtr<NSUInteger>)indexes length:(NSUInteger)length {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithIndexes:indexes length:length];
    autorelease(env, new)
}
// UIKit addition.
+ (id)indexPathForRow:(NSInteger)row inSection:(NSInteger)section {
    new_index_path(env, &[section as NSUInteger, row as NSUInteger])
}

- (id)initWithIndex:(NSUInteger)index {
    env.objc.borrow_mut::<NSIndexPathHostObject>(this).indexes = vec![index];
    this
}
- (id)initWithIndexes:(ConstPtr<NSUInteger>)indexes length:(NSUInteger)length {
    let indexes = (0..length).map(|i| env.mem.read(indexes + i)).collect();
    env.objc.borrow_mut::<NSIndexPathHostObject>(this).indexes = indexes;
    this
}

- (NSUInteger)length {
    env.objc.borrow::<NSIndexPathHostObject>(this).indexes.len() as NSUInteger
}
- (NSUInteger)indexAtPosition:(NSUInteger)position {
    let indexes = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    indexes
        .get(position as usize)
        .copied()
        .unwrap_or(NSNotFound as NSUInteger)
}
- (())getIndexes:(MutPtr<NSUInteger>)indexes {
    let values = env.objc.borrow::<NSIndexPathHostObject>(this).indexes.clone();
    for (i, value) in values.into_iter().enumerate() {
        env.mem.write(indexes + i as NSUInteger, value);
    }
}
- (id)indexPathByAddingIndex:(NSUInteger)index {
    let mut indexes = env.objc.borrow::<NSIndexPathHostObject>(this).indexes.clone();
    indexes.push(index);
    new_index_path(env, &indexes)
}
- (id)indexPathByRemovingLastIndex {
    let mut indexes = env.objc.borrow::<NSIndexPathHostObject>(this).indexes.clone();
    indexes.pop();
    new_index_path(env, &indexes)
}

// UIKit additions.
- (NSInteger)section {
    let index: NSUInteger = msg![env; this indexAtPosition:0u32];
    index as NSInteger
}
- (NSInteger)row {
    let index: NSUInteger = msg![env; this indexAtPosition:1u32];
    index as NSInteger
}

- (NSComparisonResult)compare:(id)other { // NSIndexPath*
    let a = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    let b = &env.objc.borrow::<NSIndexPathHostObject>(other).indexes;
    from_rust_ordering(a.cmp(b))
}
- (bool)isEqual:(id)other {
    let class: Class = msg_class![env; NSIndexPath class];
    if !msg![env; other isKindOfClass:class] {
        return false;
    }
    let res: NSComparisonResult = msg![env; this compare:other];
    res == 0
}
- (NSUInteger)hash {
    let indexes = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    indexes
        .iter()
        .fold(indexes.len() as NSUInteger, |hash, &index| {
            hash.wrapping_mul(31).wrapping_add(index)
        })
}

// Index paths are immutable.
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

@end

};

/// An autoreleased `NSIndexPath*` with the given indexes.
pub fn new_index_path(env: &mut Environment, indexes: &[NSUInteger]) -> id {
    let new: id = msg_class![env; NSIndexPath alloc];
    env.objc.borrow_mut::<NSIndexPathHostObject>(new).indexes = indexes.to_vec();
    autorelease(env, new)
}
