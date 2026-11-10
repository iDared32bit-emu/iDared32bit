//! `UIGeometry.h`
//!
//! See also [crate::frameworks::core_graphics::cg_geometry].

use crate::abi::GuestArg;
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string;
use crate::objc::{autorelease, id, nil};
use crate::Environment;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C, packed)]
pub struct UIEdgeInsets {
    pub top: CGFloat,
    pub left: CGFloat,
    pub bottom: CGFloat,
    pub right: CGFloat,
}
impl GuestArg for UIEdgeInsets {
    const REG_COUNT: usize = 4;

    fn from_regs(regs: &[u32]) -> Self {
        UIEdgeInsets {
            top: GuestArg::from_regs(&regs[0..1]),
            left: GuestArg::from_regs(&regs[1..2]),
            bottom: GuestArg::from_regs(&regs[2..3]),
            right: GuestArg::from_regs(&regs[3..4]),
        }
    }
    fn to_regs(self, regs: &mut [u32]) {
        self.top.to_regs(&mut regs[0..1]);
        self.left.to_regs(&mut regs[1..2]);
        self.bottom.to_regs(&mut regs[2..3]);
        self.right.to_regs(&mut regs[3..4]);
    }
}

// Apple's documentation says all of these return zeroes if the input is not
// well-formed.
pub fn CGPointFromString(env: &mut Environment, string: id) -> CGPoint {
    if string == nil {
        return Default::default();
    }
    // TODO: avoid copy
    ns_string::to_rust_string(env, string)
        .parse()
        .unwrap_or_default()
}
pub fn CGSizeFromString(env: &mut Environment, string: id) -> CGSize {
    if string == nil {
        return Default::default();
    }
    // TODO: avoid copy
    ns_string::to_rust_string(env, string)
        .parse()
        .unwrap_or_default()
}
pub fn CGRectFromString(env: &mut Environment, string: id) -> CGRect {
    if string == nil {
        return Default::default();
    }
    // TODO: avoid copy
    ns_string::to_rust_string(env, string)
        .parse()
        .unwrap_or_default()
}

pub fn NSStringFromCGPoint(env: &mut Environment, point: CGPoint) -> id {
    let s = ns_string::from_rust_string(env, point.to_string());
    autorelease(env, s)
}
pub fn NSStringFromCGSize(env: &mut Environment, size: CGSize) -> id {
    let s = ns_string::from_rust_string(env, size.to_string());
    autorelease(env, s)
}
pub fn NSStringFromCGRect(env: &mut Environment, rect: CGRect) -> id {
    let s = ns_string::from_rust_string(env, rect.to_string());
    autorelease(env, s)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGPointFromString(_)),
    export_c_func!(CGSizeFromString(_)),
    export_c_func!(CGRectFromString(_)),
    export_c_func!(NSStringFromCGPoint(_)),
    export_c_func!(NSStringFromCGSize(_)),
    export_c_func!(NSStringFromCGRect(_)),
];
