/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSIndexSet` and `NSMutableIndexSet`.

use super::{NSNotFound, NSRange, NSUInteger};
use crate::mem::MutPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;
use std::ops::Range;

/// Sorted, non-overlapping, non-adjacent ranges. u64, so that the end of a
/// range ending at the largest index doesn't overflow.
#[derive(Default, Clone, PartialEq)]
struct NSIndexSetHostObject {
    ranges: Vec<Range<u64>>,
}
impl HostObject for NSIndexSetHostObject {}

impl NSIndexSetHostObject {
    fn add(&mut self, range: Range<u64>) {
        if range.is_empty() {
            return;
        }
        let (mut start, mut end) = (range.start, range.end);
        // Merge with every range that overlaps or touches the new one.
        self.ranges.retain(|r| {
            if r.end < start || r.start > end {
                true
            } else {
                start = start.min(r.start);
                end = end.max(r.end);
                false
            }
        });
        let at = self.ranges.partition_point(|r| r.start < start);
        self.ranges.insert(at, start..end);
    }
    fn remove(&mut self, range: Range<u64>) {
        let mut new = Vec::with_capacity(self.ranges.len() + 1);
        for r in self.ranges.drain(..) {
            if r.end <= range.start || r.start >= range.end {
                new.push(r);
                continue;
            }
            if r.start < range.start {
                new.push(r.start..range.start);
            }
            if r.end > range.end {
                new.push(range.end..r.end);
            }
        }
        self.ranges = new;
    }
    fn contains(&self, index: u64) -> bool {
        self.ranges.iter().any(|r| r.contains(&index))
    }
    fn count(&self) -> u64 {
        self.ranges.iter().map(|r| r.end - r.start).sum()
    }
    fn count_in(&self, range: &Range<u64>) -> u64 {
        self.ranges
            .iter()
            .map(|r| {
                r.end
                    .min(range.end)
                    .saturating_sub(r.start.max(range.start))
            })
            .sum()
    }
    /// The smallest index that's at least `index`, if any.
    fn at_least(&self, index: u64) -> Option<u64> {
        self.ranges
            .iter()
            .find(|r| r.end > index)
            .map(|r| r.start.max(index))
    }
    /// The largest index that's at most `index`, if any.
    fn at_most(&self, index: u64) -> Option<u64> {
        self.ranges
            .iter()
            .rev()
            .find(|r| r.start <= index)
            .map(|r| (r.end - 1).min(index))
    }
}

fn not_found_if_none(index: Option<u64>) -> NSUInteger {
    index.map_or(NSNotFound as NSUInteger, |i| i as NSUInteger)
}

fn ns_range_to_range(range: NSRange) -> Range<u64> {
    let start = range.location as u64;
    start..start + range.length as u64
}

fn host(env: &Environment, this: id) -> &NSIndexSetHostObject {
    env.objc.borrow(this)
}
fn host_mut(env: &mut Environment, this: id) -> &mut NSIndexSetHostObject {
    env.objc.borrow_mut(this)
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSIndexSet: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSIndexSetHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)indexSet {
    let new: id = msg![env; this new];
    autorelease(env, new)
}
+ (id)indexSetWithIndex:(NSUInteger)index {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithIndex:index];
    autorelease(env, new)
}
+ (id)indexSetWithIndexesInRange:(NSRange)range {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithIndexesInRange:range];
    autorelease(env, new)
}

- (id)initWithIndex:(NSUInteger)index {
    host_mut(env, this).add(index as u64..index as u64 + 1);
    this
}
- (id)initWithIndexesInRange:(NSRange)range {
    host_mut(env, this).add(ns_range_to_range(range));
    this
}
- (id)initWithIndexSet:(id)other { // NSIndexSet*
    if other != nil {
        let ranges = host(env, other).ranges.clone();
        host_mut(env, this).ranges = ranges;
    }
    this
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    let new: id = msg_class![env; NSIndexSet alloc];
    msg![env; new initWithIndexSet:this]
}
// NSMutableCopying implementation
- (id)mutableCopyWithZone:(NSZonePtr)_zone {
    let new: id = msg_class![env; NSMutableIndexSet alloc];
    msg![env; new initWithIndexSet:this]
}

- (NSUInteger)count {
    host(env, this).count() as NSUInteger
}
- (NSUInteger)countOfIndexesInRange:(NSRange)range {
    host(env, this).count_in(&ns_range_to_range(range)) as NSUInteger
}
- (NSUInteger)firstIndex {
    not_found_if_none(host(env, this).ranges.first().map(|r| r.start))
}
- (NSUInteger)lastIndex {
    not_found_if_none(host(env, this).ranges.last().map(|r| r.end - 1))
}
- (NSUInteger)indexGreaterThanIndex:(NSUInteger)index {
    not_found_if_none(host(env, this).at_least(index as u64 + 1))
}
- (NSUInteger)indexGreaterThanOrEqualToIndex:(NSUInteger)index {
    not_found_if_none(host(env, this).at_least(index as u64))
}
- (NSUInteger)indexLessThanIndex:(NSUInteger)index {
    let index = (index as u64).checked_sub(1);
    not_found_if_none(index.and_then(|index| host(env, this).at_most(index)))
}
- (NSUInteger)indexLessThanOrEqualToIndex:(NSUInteger)index {
    not_found_if_none(host(env, this).at_most(index as u64))
}

- (bool)containsIndex:(NSUInteger)index {
    host(env, this).contains(index as u64)
}
- (bool)containsIndexesInRange:(NSRange)range {
    let range = ns_range_to_range(range);
    !range.is_empty() && host(env, this).count_in(&range) == range.end - range.start
}
- (bool)containsIndexes:(id)other { // NSIndexSet*
    let other = host(env, other).ranges.clone();
    let this = host(env, this);
    other.iter().all(|r| this.count_in(r) == r.end - r.start)
}
- (bool)intersectsIndexesInRange:(NSRange)range {
    host(env, this).count_in(&ns_range_to_range(range)) > 0
}

- (NSUInteger)getIndexes:(MutPtr<NSUInteger>)buffer
                maxCount:(NSUInteger)max_count
            inIndexRange:(MutPtr<NSRange>)range_ptr { // NSRangePointer
    let limit = if range_ptr.is_null() {
        0..u64::MAX
    } else {
        ns_range_to_range(env.mem.read(range_ptr))
    };
    let indexes: Vec<u64> = host(env, this)
        .ranges
        .iter()
        .flat_map(|r| r.start.max(limit.start)..r.end.min(limit.end))
        .take(max_count as usize)
        .collect();
    for (i, &index) in indexes.iter().enumerate() {
        env.mem.write(buffer + i as NSUInteger, index as NSUInteger);
    }
    // The range is updated to start after the last index returned.
    if !range_ptr.is_null() {
        if let Some(&last) = indexes.last() {
            let start = (last + 1).min(limit.end);
            env.mem.write(range_ptr, NSRange {
                location: start as NSUInteger,
                length: (limit.end - start) as NSUInteger,
            });
        }
    }
    indexes.len() as NSUInteger
}

- (bool)isEqualToIndexSet:(id)other { // NSIndexSet*
    other != nil && host(env, this) == host(env, other)
}
- (bool)isEqual:(id)other {
    let class: id = msg_class![env; NSIndexSet class];
    other != nil && msg![env; other isKindOfClass:class] && host(env, this) == host(env, other)
}
- (NSUInteger)hash {
    host(env, this).count() as NSUInteger
}

@end

@implementation NSMutableIndexSet: NSIndexSet

- (id)copyWithZone:(NSZonePtr)_zone {
    let new: id = msg_class![env; NSIndexSet alloc];
    msg![env; new initWithIndexSet:this]
}

- (())addIndex:(NSUInteger)index {
    host_mut(env, this).add(index as u64..index as u64 + 1);
}
- (())addIndexesInRange:(NSRange)range {
    host_mut(env, this).add(ns_range_to_range(range));
}
- (())addIndexes:(id)other { // NSIndexSet*
    let ranges = host(env, other).ranges.clone();
    for range in ranges {
        host_mut(env, this).add(range);
    }
}
- (())removeIndex:(NSUInteger)index {
    host_mut(env, this).remove(index as u64..index as u64 + 1);
}
- (())removeIndexesInRange:(NSRange)range {
    host_mut(env, this).remove(ns_range_to_range(range));
}
- (())removeIndexes:(id)other { // NSIndexSet*
    let ranges = host(env, other).ranges.clone();
    for range in ranges {
        host_mut(env, this).remove(range);
    }
}
- (())removeAllIndexes {
    host_mut(env, this).ranges.clear();
}
- (())shiftIndexesStartingAtIndex:(NSUInteger)index by:(i32)delta {
    // Indexes from `index` on move by `delta`. Moving down, the ones they'd
    // land on before `index` are removed first.
    let index = index as u64;
    let host_object = host_mut(env, this);
    if delta < 0 {
        let gap = (-delta) as u64;
        host_object.remove(index.saturating_sub(gap)..index);
    }
    let moved: Vec<Range<u64>> = host_object
        .ranges
        .iter()
        .filter(|r| r.end > index)
        .map(|r| r.start.max(index)..r.end)
        .collect();
    host_object.remove(index..u64::MAX);
    for r in moved {
        let shift = |i: u64| (i as i64 + delta as i64).max(0) as u64;
        host_object.add(shift(r.start)..shift(r.end));
    }
}

@end

};

#[cfg(test)]
mod tests {
    use super::NSIndexSetHostObject;

    fn set(ranges: &[std::ops::Range<u64>]) -> NSIndexSetHostObject {
        let mut set = NSIndexSetHostObject::default();
        for r in ranges {
            set.add(r.clone());
        }
        set
    }

    #[test]
    fn add_merges_overlapping_and_adjacent_ranges() {
        assert_eq!(set(&[5..7, 1..3, 3..4, 6..10]).ranges, [1..4, 5..10]);
        assert_eq!(set(&[1..2, 5..6, 0..10]).ranges, [0..10]);
    }

    #[test]
    fn remove_splits_ranges() {
        let mut s = set(&[0..10]);
        s.remove(3..5);
        assert_eq!(s.ranges, [0..3, 5..10]);
        s.remove(0..1);
        s.remove(9..20);
        assert_eq!(s.ranges, [1..3, 5..9]);
        assert_eq!(s.count(), 6);
    }

    #[test]
    fn neighbours() {
        let s = set(&[2..4, 8..9]);
        assert_eq!(s.at_least(4), Some(8));
        assert_eq!(s.at_least(3), Some(3));
        assert_eq!(s.at_least(9), None);
        assert_eq!(s.at_most(7), Some(3));
        assert_eq!(s.at_most(1), None);
        assert_eq!(s.count_in(&(3..9)), 2);
    }
}
