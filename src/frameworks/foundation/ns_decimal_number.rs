/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSDecimalNumber`.
//!
//! The exact decimal is kept for its text and comparisons. NSNumber's
//! accessors (`doubleValue`, `intValue`, …) use the nearest double, as iOS's
//! do. Arithmetic (`decimalNumberByAdding:` etc.) isn't implemented.

use super::ns_string::{from_rust_ordering, from_rust_string, to_rust_string};
use super::ns_value::NSNumberHostObject;
use super::NSComparisonResult;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, objc_classes, ClassExports, NSZonePtr,
};
use std::cmp::Ordering;

/// A decimal: `mantissa` × 10^`exponent`, as on iOS up to 38 digits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decimal {
    negative: bool,
    mantissa: u128,
    exponent: i32,
}

const MAX_DIGITS: usize = 38;

impl Decimal {
    /// Like `decimalNumberWithString:`: an optional sign, digits with an
    /// optional point, then an optional exponent. Anything after that is
    /// ignored. [None] (not a number) if there are no digits.
    pub fn parse(text: &str) -> Option<Decimal> {
        let mut chars = text.trim_start().chars().peekable();
        let negative = match chars.peek() {
            Some('-') => {
                chars.next();
                true
            }
            Some('+') => {
                chars.next();
                false
            }
            _ => false,
        };
        let (mut mantissa, mut exponent, mut digits) = (0u128, 0i32, 0usize);
        let (mut any_digits, mut after_point) = (false, false);
        while let Some(&c) = chars.peek() {
            if c == '.' && !after_point {
                after_point = true;
            } else if let Some(digit) = c.to_digit(10) {
                any_digits = true;
                // Extra digits are dropped, as on iOS.
                if digits < MAX_DIGITS {
                    if mantissa != 0 || digit != 0 {
                        digits += 1;
                    }
                    mantissa = mantissa * 10 + u128::from(digit);
                    if after_point {
                        exponent -= 1;
                    }
                } else if !after_point {
                    exponent += 1;
                }
            } else {
                break;
            }
            chars.next();
        }
        if !any_digits {
            return None;
        }
        if matches!(chars.peek(), Some('e' | 'E')) {
            chars.next();
            let rest: String = chars.collect();
            let end = rest
                .char_indices()
                .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
                .map_or(rest.len(), |(i, _)| i);
            if let Ok(e) = rest[..end].parse::<i32>() {
                exponent = exponent.saturating_add(e);
            }
        }
        Some(
            Decimal {
                negative,
                mantissa,
                exponent,
            }
            .normalized(),
        )
    }

    pub fn new(mantissa: u64, exponent: i16, negative: bool) -> Decimal {
        Decimal {
            negative,
            mantissa: mantissa.into(),
            exponent: exponent.into(),
        }
        .normalized()
    }

    /// Without trailing zeros, so equal values look the same.
    fn normalized(mut self) -> Decimal {
        if self.mantissa == 0 {
            return Decimal {
                negative: false,
                mantissa: 0,
                exponent: 0,
            };
        }
        while self.mantissa.is_multiple_of(10) {
            self.mantissa /= 10;
            self.exponent += 1;
        }
        self
    }

    pub fn to_f64(self) -> f64 {
        let sign = if self.negative { "-" } else { "" };
        format!("{}{}e{}", sign, self.mantissa, self.exponent)
            .parse()
            .unwrap()
    }

    /// As iOS writes it: plain digits, never an exponent.
    pub fn to_text(self) -> String {
        let digits = self.mantissa.to_string();
        let sign = if self.negative { "-" } else { "" };
        if self.exponent >= 0 {
            let zeros = "0".repeat(self.exponent as usize);
            return format!("{sign}{digits}{zeros}");
        }
        let point = digits.len() as i64 + i64::from(self.exponent);
        if point > 0 {
            let (whole, fraction) = digits.split_at(point as usize);
            format!("{sign}{whole}.{fraction}")
        } else {
            let zeros = "0".repeat((-point) as usize);
            format!("{sign}0.{zeros}{digits}")
        }
    }

    fn compare(self, other: Decimal) -> Ordering {
        if self.negative != other.negative {
            return if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        // Line up the exponents; digits beyond u128 can't differ here.
        let exponent = self.exponent.min(other.exponent);
        let scale = |d: Decimal| {
            let shift = (d.exponent - exponent).min(40) as u32;
            d.mantissa.checked_mul(10u128.pow(shift.min(38)))
        };
        let ordering = match (scale(self), scale(other)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => self.to_f64().abs().total_cmp(&other.to_f64().abs()),
        };
        if self.negative {
            ordering.reverse()
        } else {
            ordering
        }
    }
}

pub struct NSDecimalNumberHostObject {
    /// The nearest double, for NSNumber's methods.
    superclass: NSNumberHostObject,
    /// [None] for not a number.
    decimal: Option<Decimal>,
}
impl_HostObject_with_superclass!(NSDecimalNumberHostObject);

fn set(env: &mut crate::Environment, this: id, decimal: Option<Decimal>) -> id {
    let value = decimal.map_or(f64::NAN, Decimal::to_f64);
    let host = env.objc.borrow_mut::<NSDecimalNumberHostObject>(this);
    host.superclass = NSNumberHostObject::Double(value);
    host.decimal = decimal;
    this
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSDecimalNumber: NSNumber

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSDecimalNumberHostObject {
        superclass: NSNumberHostObject::Double(0.0),
        decimal: Some(Decimal::new(0, 0, false)),
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)decimalNumberWithString:(id)string { // NSString*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithString:string];
    autorelease(env, new)
}

+ (id)decimalNumberWithMantissa:(u64)mantissa
                       exponent:(i16)exponent
                     isNegative:(bool)negative {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithMantissa:mantissa
                                        exponent:exponent
                                      isNegative:negative];
    autorelease(env, new)
}

+ (id)notANumber {
    let new: id = msg![env; this alloc];
    let new = set(env, new, None);
    autorelease(env, new)
}

- (id)initWithString:(id)string { // NSString*
    let decimal = if string == crate::objc::nil {
        None
    } else {
        Decimal::parse(&to_rust_string(env, string))
    };
    set(env, this, decimal)
}

- (id)initWithMantissa:(u64)mantissa
              exponent:(i16)exponent
            isNegative:(bool)negative {
    set(env, this, Some(Decimal::new(mantissa, exponent, negative)))
}

// NSNumber's description and stringValue come here.
- (id)descriptionWithLocale:(id)_locale {
    let decimal = env.objc.borrow::<NSDecimalNumberHostObject>(this).decimal;
    let text = decimal.map_or("NaN".to_string(), Decimal::to_text);
    let string = from_rust_string(env, text);
    autorelease(env, string)
}

// Exact between decimal numbers. Not a number is less than any number.
- (NSComparisonResult)compare:(id)other { // NSNumber*
    let class = env.objc.get_known_class("NSDecimalNumber", &mut env.mem);
    let other_class = msg![env; other class];
    let this_decimal = env.objc.borrow::<NSDecimalNumberHostObject>(this).decimal;
    let other_decimal = if env.objc.class_is_subclass_of(other_class, class) {
        env.objc.borrow::<NSDecimalNumberHostObject>(other).decimal
    } else {
        let value: f64 = msg![env; other doubleValue];
        Decimal::parse(&format!("{value:e}"))
    };
    let ordering = match (this_decimal, other_decimal) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => a.compare(b),
    };
    from_rust_ordering(ordering)
}

@end

};

#[cfg(test)]
mod tests {
    use super::Decimal;

    #[test]
    fn parse_and_text() {
        let text = |s: &str| Decimal::parse(s).map(Decimal::to_text);
        assert_eq!(text("1.50"), Some("1.5".to_string()));
        assert_eq!(text("-0.025"), Some("-0.025".to_string()));
        assert_eq!(text("12e3"), Some("12000".to_string()));
        assert_eq!(text("  +7.25e-2xyz"), Some("0.0725".to_string()));
        assert_eq!(text("0.000"), Some("0".to_string()));
        assert_eq!(text("abc"), None);
        assert_eq!(text("."), None);
        assert_eq!(Decimal::parse("0.1").unwrap().to_f64(), 0.1);
        assert_eq!(Decimal::new(314, -2, true).to_text(), "-3.14");
    }

    #[test]
    fn compare() {
        let d = |s: &str| Decimal::parse(s).unwrap();
        assert!(d("1.5").compare(d("1.50")).is_eq());
        assert!(d("-2").compare(d("1")).is_lt());
        assert!(d("-2").compare(d("-10")).is_gt());
        assert!(d("0.1").compare(d("0.09")).is_gt());
    }
}
