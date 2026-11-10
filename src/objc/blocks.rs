/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The Blocks runtime (iOS 4+): `_Block_copy` and friends, and the classes
//! blocks belong to.
//!
//! Resources:
//! - [Block Implementation Specification](https://clang.llvm.org/docs/Block-ABI-Apple.html)
//!
//! A block is a struct the compiler lays out (on the stack, or as a global
//! constant for blocks that capture nothing): isa, flags, reserved, invoke,
//! descriptor, then the captured variables. The descriptor holds the
//! block's size and, with `BLOCK_HAS_COPY_DISPOSE`, helpers that retain and
//! release what it captured. `__block` variables live in a similar "byref"
//! struct that moves to the heap with the first block copied that uses it.
//!
//! The reference count of heap blocks and byrefs is kept in the low 16 bits
//! of their flags, which only this runtime reads.

use super::{id, objc_classes, release, retain, ClassExports, NSZonePtr};
use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::frameworks::foundation::NSUInteger;
use crate::mem::{ConstVoidPtr, GuestUSize, MutPtr, Ptr};
use crate::Environment;

const BLOCK_REFCOUNT_MASK: u32 = 0xffff;
const BLOCK_NEEDS_FREE: u32 = 1 << 24;
const BLOCK_HAS_COPY_DISPOSE: u32 = 1 << 25;
const BLOCK_IS_GLOBAL: u32 = 1 << 28;

// Flags for _Block_object_assign() and _Block_object_dispose().
const BLOCK_FIELD_IS_OBJECT: i32 = 3;
const BLOCK_FIELD_IS_BLOCK: i32 = 7;
const BLOCK_FIELD_IS_BYREF: i32 = 8;
const BLOCK_FIELD_IS_WEAK: i32 = 16;
const BLOCK_BYREF_CALLER: i32 = 128;

// Offsets in a block: isa, flags, reserved, invoke, descriptor.
const BLOCK_ISA: GuestUSize = 0;
const BLOCK_FLAGS: GuestUSize = 4;
const BLOCK_DESCRIPTOR: GuestUSize = 16;
// Offsets in a descriptor: reserved, size, copy helper, dispose helper.
const DESCRIPTOR_SIZE: GuestUSize = 4;
const DESCRIPTOR_COPY: GuestUSize = 8;
const DESCRIPTOR_DISPOSE: GuestUSize = 12;
// Offsets in a byref: isa, forwarding, flags, size, keep, destroy.
const BYREF_ISA: GuestUSize = 0;
const BYREF_FORWARDING: GuestUSize = 4;
const BYREF_FLAGS: GuestUSize = 8;
const BYREF_SIZE: GuestUSize = 12;
const BYREF_KEEP: GuestUSize = 16;
const BYREF_DESTROY: GuestUSize = 20;

fn field(ptr: ConstVoidPtr, offset: GuestUSize) -> MutPtr<u32> {
    Ptr::from_bits(ptr.to_bits() + offset)
}
fn read(env: &Environment, ptr: ConstVoidPtr, offset: GuestUSize) -> u32 {
    env.mem.read(field(ptr, offset))
}
fn write(env: &mut Environment, ptr: ConstVoidPtr, offset: GuestUSize, value: u32) {
    env.mem.write(field(ptr, offset), value)
}
fn call_helper(env: &mut Environment, helper: u32, args: (ConstVoidPtr, ConstVoidPtr)) {
    let helper = GuestFunction::from_addr_with_thumb_bit(helper);
    () = helper.call_from_host(env, args);
}
fn call_helper_1(env: &mut Environment, helper: u32, arg: ConstVoidPtr) {
    let helper = GuestFunction::from_addr_with_thumb_bit(helper);
    () = helper.call_from_host(env, (arg,));
}

/// Adds one to a heap block's or byref's reference count.
fn increment(env: &mut Environment, ptr: ConstVoidPtr, flags_offset: GuestUSize) {
    let flags = read(env, ptr, flags_offset);
    assert!(flags & BLOCK_REFCOUNT_MASK != BLOCK_REFCOUNT_MASK);
    write(env, ptr, flags_offset, flags + 1);
}
/// Takes one from a heap block's or byref's reference count, and returns
/// whether it reached zero.
fn decrement(env: &mut Environment, ptr: ConstVoidPtr, flags_offset: GuestUSize) -> bool {
    let flags = read(env, ptr, flags_offset);
    assert!(
        flags & BLOCK_REFCOUNT_MASK != 0,
        "Over-released block {ptr:?}"
    );
    write(env, ptr, flags_offset, flags - 1);
    flags & BLOCK_REFCOUNT_MASK == 1
}

/// Copies a stack block to the heap. Heap blocks are retained instead, and
/// global blocks returned as they are.
pub fn _Block_copy(env: &mut Environment, block: ConstVoidPtr) -> ConstVoidPtr {
    if block.is_null() {
        return block;
    }
    let flags = read(env, block, BLOCK_FLAGS);
    if flags & BLOCK_NEEDS_FREE != 0 {
        increment(env, block, BLOCK_FLAGS);
        return block;
    }
    if flags & BLOCK_IS_GLOBAL != 0 {
        return block;
    }
    let descriptor = Ptr::from_bits(read(env, block, BLOCK_DESCRIPTOR));
    let size = read(env, descriptor, DESCRIPTOR_SIZE);
    let copy = env.mem.alloc(size).cast_const();
    env.mem.memmove(copy.cast_mut(), block, size);
    let flags = (flags & !BLOCK_REFCOUNT_MASK) | BLOCK_NEEDS_FREE | 1;
    write(env, copy, BLOCK_FLAGS, flags);
    let malloc_block = env.objc.get_known_class("__NSMallocBlock__", &mut env.mem);
    write(env, copy, BLOCK_ISA, malloc_block.to_bits());
    if flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let copy_helper = read(env, descriptor, DESCRIPTOR_COPY);
        call_helper(env, copy_helper, (copy, block));
    }
    copy
}

/// Releases a heap block. Stack and global blocks aren't reference counted.
pub fn _Block_release(env: &mut Environment, block: ConstVoidPtr) {
    if block.is_null() {
        return;
    }
    let flags = read(env, block, BLOCK_FLAGS);
    if flags & BLOCK_IS_GLOBAL != 0 || flags & BLOCK_NEEDS_FREE == 0 {
        return;
    }
    if !decrement(env, block, BLOCK_FLAGS) {
        return;
    }
    if flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let descriptor = Ptr::from_bits(read(env, block, BLOCK_DESCRIPTOR));
        let dispose_helper = read(env, descriptor, DESCRIPTOR_DISPOSE);
        call_helper_1(env, dispose_helper, block);
    }
    env.mem.free(block.cast_mut());
}

/// Moves a `__block` variable to the heap, the first time a block using it
/// is copied. Later copies share the heap one, which is retained.
fn byref_copy(env: &mut Environment, byref: ConstVoidPtr) -> ConstVoidPtr {
    let forwarding = Ptr::from_bits(read(env, byref, BYREF_FORWARDING));
    if read(env, forwarding, BYREF_FLAGS) & BLOCK_NEEDS_FREE != 0 {
        increment(env, forwarding, BYREF_FLAGS);
        return forwarding;
    }
    let flags = read(env, byref, BYREF_FLAGS);
    let size = read(env, byref, BYREF_SIZE);
    let copy = env.mem.alloc(size).cast_const();
    env.mem.memmove(copy.cast_mut(), byref, size);
    write(env, copy, BYREF_ISA, 0);
    // One reference for the block, one for the stack frame's own dispose.
    let flags = (flags & !BLOCK_REFCOUNT_MASK) | BLOCK_NEEDS_FREE | 2;
    write(env, copy, BYREF_FLAGS, flags);
    write(env, copy, BYREF_FORWARDING, copy.to_bits());
    write(env, byref, BYREF_FORWARDING, copy.to_bits());
    if flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let keep = read(env, byref, BYREF_KEEP);
        call_helper(env, keep, (copy, byref));
    }
    copy
}

fn byref_release(env: &mut Environment, byref: ConstVoidPtr) {
    let byref = Ptr::from_bits(read(env, byref, BYREF_FORWARDING));
    let flags = read(env, byref, BYREF_FLAGS);
    if flags & BLOCK_NEEDS_FREE == 0 || !decrement(env, byref, BYREF_FLAGS) {
        return;
    }
    if flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let destroy = read(env, byref, BYREF_DESTROY);
        call_helper_1(env, destroy, byref);
    }
    env.mem.free(byref.cast_mut());
}

/// Called by blocks' copy helpers for each captured object, block or
/// `__block` variable, and by byrefs' keep helpers.
fn _Block_object_assign(
    env: &mut Environment,
    dest: MutPtr<u32>,
    object: ConstVoidPtr,
    flags: i32,
) {
    let value = if flags & BLOCK_BYREF_CALLER != 0 {
        // A byref's own copy of an object or block: not retained.
        object
    } else {
        match flags & !BLOCK_FIELD_IS_WEAK {
            BLOCK_FIELD_IS_BYREF => byref_copy(env, object),
            BLOCK_FIELD_IS_BLOCK => _Block_copy(env, object),
            BLOCK_FIELD_IS_OBJECT => retain(env, object.cast_mut().cast()).cast().cast_const(),
            _ => unimplemented!("_Block_object_assign() with flags {flags:#x}"),
        }
    };
    env.mem.write(dest, value.to_bits());
}

/// Called by blocks' dispose helpers and byrefs' destroy helpers, and when
/// a stack frame with a `__block` variable ends.
fn _Block_object_dispose(env: &mut Environment, object: ConstVoidPtr, flags: i32) {
    if flags & BLOCK_BYREF_CALLER != 0 {
        return;
    }
    match flags & !BLOCK_FIELD_IS_WEAK {
        BLOCK_FIELD_IS_BYREF => byref_release(env, object),
        BLOCK_FIELD_IS_BLOCK => _Block_release(env, object),
        BLOCK_FIELD_IS_OBJECT => release(env, object.cast_mut().cast()),
        _ => unimplemented!("_Block_object_dispose() with flags {flags:#x}"),
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// Blocks are objects too. They aren't in the object table, so the memory
// management methods are the Blocks runtime's. autorelease is NSObject's.
@implementation NSBlock: NSObject

- (id)copy {
    _Block_copy(env, this.cast().cast_const()).cast_mut().cast()
}
- (id)copyWithZone:(NSZonePtr)_zone {
    _Block_copy(env, this.cast().cast_const()).cast_mut().cast()
}
- (id)retain {
    // Only heap blocks are reference counted; retaining a stack block
    // doesn't copy it.
    if read(env, this.cast().cast_const(), BLOCK_FLAGS) & BLOCK_NEEDS_FREE != 0 {
        increment(env, this.cast().cast_const(), BLOCK_FLAGS);
    }
    this
}
- (())release {
    _Block_release(env, this.cast().cast_const())
}
- (NSUInteger)retainCount {
    let flags = read(env, this.cast().cast_const(), BLOCK_FLAGS);
    if flags & BLOCK_NEEDS_FREE != 0 {
        flags & BLOCK_REFCOUNT_MASK
    } else {
        NSUInteger::MAX
    }
}

@end

@implementation __NSGlobalBlock__: NSBlock
@end
@implementation __NSStackBlock__: NSBlock
@end
@implementation __NSMallocBlock__: NSBlock
@end

};

/// The isa of each kind of block is the address of one of these symbols,
/// which are the classes themselves.
pub const CONSTANTS: ConstantExports = &[
    (
        "__NSConcreteGlobalBlock",
        HostConstant::Custom(|env| {
            let class = env.objc.get_known_class("__NSGlobalBlock__", &mut env.mem);
            class.cast().cast_const()
        }),
    ),
    (
        "__NSConcreteStackBlock",
        HostConstant::Custom(|env| {
            let class = env.objc.get_known_class("__NSStackBlock__", &mut env.mem);
            class.cast().cast_const()
        }),
    ),
    (
        "__NSConcreteMallocBlock",
        HostConstant::Custom(|env| {
            let class = env.objc.get_known_class("__NSMallocBlock__", &mut env.mem);
            class.cast().cast_const()
        }),
    ),
];

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(_Block_copy(_)),
    export_c_func!(_Block_release(_)),
    export_c_func!(_Block_object_assign(_, _, _)),
    export_c_func!(_Block_object_dispose(_, _)),
];
