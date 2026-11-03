/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSCalendar` and `NSDateComponents`. Only the Gregorian calendar.

use super::{ns_string, NSInteger, NSTimeInterval, NSUInteger};
use crate::dyld::{ConstantExports, HostConstant};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;

pub const NSGregorianCalendar: &str = "gregorian";

pub const CONSTANTS: ConstantExports = &[(
    "_NSGregorianCalendar",
    HostConstant::NSString(NSGregorianCalendar),
)];

type NSCalendarUnit = NSUInteger;
const NSEraCalendarUnit: NSCalendarUnit = 1 << 1;
const NSYearCalendarUnit: NSCalendarUnit = 1 << 2;
const NSMonthCalendarUnit: NSCalendarUnit = 1 << 3;
const NSDayCalendarUnit: NSCalendarUnit = 1 << 4;
const NSHourCalendarUnit: NSCalendarUnit = 1 << 5;
const NSMinuteCalendarUnit: NSCalendarUnit = 1 << 6;
const NSSecondCalendarUnit: NSCalendarUnit = 1 << 7;
const NSWeekCalendarUnit: NSCalendarUnit = 1 << 8;
const NSWeekdayCalendarUnit: NSCalendarUnit = 1 << 9;
const NSWeekdayOrdinalCalendarUnit: NSCalendarUnit = 1 << 10;

/// A component that isn't set.
const NSUndefinedDateComponent: NSInteger = NSInteger::MAX;

/// For `options` that would wrap rather than carry over.
const NSWrapCalendarComponents: NSUInteger = 1;

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

/// The inverse of [days_from_civil]: (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719468;
    let era = days.div_euclid(146097);
    let day_of_era = days - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    next - days_from_civil(year, month, 1)
}

/// Seconds from 1970 to 2001, `NSDate`'s reference date.
const REFERENCE_DATE_FROM_UNIX: i64 = 978307200;

/// A date's fields in local time.
#[derive(Clone, Copy)]
struct Fields {
    year: i64,
    month: i64,
    day: i64,
    /// Seconds into the day, with any fraction.
    time: f64,
    /// 1 is Sunday.
    weekday: i64,
}

fn to_fields(local: NSTimeInterval) -> Fields {
    let unix = local + REFERENCE_DATE_FROM_UNIX as f64;
    let days = (unix / 86400.0).floor() as i64;
    let (year, month, day) = civil_from_days(days);
    Fields {
        year,
        month,
        day,
        time: unix - days as f64 * 86400.0,
        // 1970-01-01 was a Thursday.
        weekday: (days + 4).rem_euclid(7) + 1,
    }
}

/// Local seconds since the reference date. The month may be out of range,
/// and the day and time may overflow: they carry over, as on iOS.
fn from_fields(year: i64, month: i64, day: i64, time: f64) -> NSTimeInterval {
    let year = year + (month - 1).div_euclid(12);
    let month = (month - 1).rem_euclid(12) + 1;
    let days = days_from_civil(year, month, 1) + day - 1;
    (days * 86400 - REFERENCE_DATE_FROM_UNIX) as f64 + time
}

/// Adds months, keeping the day within the new month, as iOS does
/// (January 31 plus a month is February 28 or 29).
fn add_months(fields: Fields, months: i64) -> NSTimeInterval {
    let total = fields.year * 12 + fields.month - 1 + months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let day = fields.day.min(days_in_month(year, month));
    from_fields(year, month, day, fields.time)
}

#[derive(Clone, Copy)]
struct NSDateComponentsHostObject {
    era: NSInteger,
    year: NSInteger,
    month: NSInteger,
    day: NSInteger,
    hour: NSInteger,
    minute: NSInteger,
    second: NSInteger,
    week: NSInteger,
    weekday: NSInteger,
    weekday_ordinal: NSInteger,
}
impl HostObject for NSDateComponentsHostObject {}

struct NSCalendarHostObject {
    /// `NSString*`
    identifier: id,
    /// `NSTimeZone*`
    time_zone: id,
}
impl HostObject for NSCalendarHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSCalendar: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSCalendarHostObject {
        identifier: nil,
        time_zone: nil,
    };
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

+ (id)currentCalendar {
    let identifier = ns_string::get_static_str(env, NSGregorianCalendar);
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithCalendarIdentifier:identifier];
    autorelease(env, new)
}
+ (id)autoupdatingCurrentCalendar {
    msg![env; this currentCalendar]
}

- (id)initWithCalendarIdentifier:(id)identifier { // NSString*
    let name = ns_string::to_rust_string(env, identifier);
    if name != NSGregorianCalendar {
        unimplemented!("Calendar {:?}", name);
    }
    let identifier: id = msg![env; identifier copy];
    let time_zone: id = msg_class![env; NSTimeZone defaultTimeZone];
    retain(env, time_zone);
    let host_object = env.objc.borrow_mut::<NSCalendarHostObject>(this);
    host_object.identifier = identifier;
    host_object.time_zone = time_zone;
    this
}

- (())dealloc {
    let &NSCalendarHostObject { identifier, time_zone } = env.objc.borrow(this);
    release(env, identifier);
    release(env, time_zone);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)calendarIdentifier {
    env.objc.borrow::<NSCalendarHostObject>(this).identifier
}

- (id)timeZone {
    env.objc.borrow::<NSCalendarHostObject>(this).time_zone
}
- (())setTimeZone:(id)time_zone { // NSTimeZone*
    retain(env, time_zone);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<NSCalendarHostObject>(this).time_zone,
        time_zone,
    );
    release(env, old);
}

- (id)components:(NSCalendarUnit)units fromDate:(id)date { // NSDate*
    let fields = to_fields(local_time(env, this, date));
    let components: id = msg_class![env; NSDateComponents new];
    let c = env.objc.borrow_mut::<NSDateComponentsHostObject>(components);
    let set = |unit, value: i64, field: &mut NSInteger| {
        if units & unit != 0 {
            *field = value as NSInteger;
        }
    };
    set(NSEraCalendarUnit, if fields.year > 0 { 1 } else { 0 }, &mut c.era);
    set(NSYearCalendarUnit, fields.year, &mut c.year);
    set(NSMonthCalendarUnit, fields.month, &mut c.month);
    set(NSDayCalendarUnit, fields.day, &mut c.day);
    let time = fields.time as i64;
    set(NSHourCalendarUnit, time / 3600, &mut c.hour);
    set(NSMinuteCalendarUnit, time / 60 % 60, &mut c.minute);
    set(NSSecondCalendarUnit, time % 60, &mut c.second);
    set(NSWeekdayCalendarUnit, fields.weekday, &mut c.weekday);
    // Which of this weekday in the month it is (the 2nd Tuesday, say).
    set(NSWeekdayOrdinalCalendarUnit, (fields.day - 1) / 7 + 1, &mut c.weekday_ordinal);
    if units & NSWeekCalendarUnit != 0 {
        log!("TODO: NSWeekCalendarUnit, left undefined");
    }
    autorelease(env, components)
}

// Unset components are the earliest they can be: January 1 of year 1, at
// midnight.
- (id)dateFromComponents:(id)components { // NSDateComponents*
    let c = *env.objc.borrow::<NSDateComponentsHostObject>(components);
    let get = |value: NSInteger, default: i64| {
        if value == NSUndefinedDateComponent {
            default
        } else {
            value as i64
        }
    };
    let time = get(c.hour, 0) * 3600 + get(c.minute, 0) * 60 + get(c.second, 0);
    let local = from_fields(get(c.year, 1), get(c.month, 1), get(c.day, 1), time as f64);
    new_date(env, this, local)
}

- (id)dateByAddingComponents:(id)components // NSDateComponents*
                      toDate:(id)date // NSDate*
                     options:(NSUInteger)options {
    if options & NSWrapCalendarComponents != 0 {
        log!("TODO: NSWrapCalendarComponents, components carry over instead");
    }
    let c = *env.objc.borrow::<NSDateComponentsHostObject>(components);
    let get = |value: NSInteger| {
        if value == NSUndefinedDateComponent {
            0
        } else {
            value as i64
        }
    };
    let fields = to_fields(local_time(env, this, date));
    let local = add_months(fields, get(c.year) * 12 + get(c.month));
    let days = get(c.week) * 7 + get(c.day);
    let seconds = days * 86400 + get(c.hour) * 3600 + get(c.minute) * 60 + get(c.second);
    new_date(env, this, local + seconds as f64)
}

// The difference in the largest units first, each rounded towards zero.
- (id)components:(NSCalendarUnit)units
        fromDate:(id)start // NSDate*
          toDate:(id)end // NSDate*
         options:(NSUInteger)_options {
    let start = local_time(env, this, start);
    let end = local_time(env, this, end);
    let start_fields = to_fields(start);
    let end_fields = to_fields(end);
    let components: id = msg_class![env; NSDateComponents new];

    // Whole months (or years), not going past the end.
    let mut months = 0;
    if units & (NSYearCalendarUnit | NSMonthCalendarUnit) != 0 {
        months = (end_fields.year * 12 + end_fields.month)
            - (start_fields.year * 12 + start_fields.month);
        while months != 0 && {
            let date = add_months(start_fields, months);
            if months > 0 { date > end } else { date < end }
        } {
            months -= months.signum();
        }
        if units & NSMonthCalendarUnit == 0 {
            months = months / 12 * 12;
        }
    }
    let mut rest = end - add_months(start_fields, months);
    let mut take = |unit_seconds: f64| {
        let count = (rest / unit_seconds).trunc();
        rest -= count * unit_seconds;
        count as NSInteger
    };
    let c = env.objc.borrow_mut::<NSDateComponentsHostObject>(components);
    if units & NSYearCalendarUnit != 0 {
        c.year = (months / 12) as NSInteger;
    }
    if units & NSMonthCalendarUnit != 0 {
        c.month = if units & NSYearCalendarUnit != 0 {
            (months % 12) as NSInteger
        } else {
            months as NSInteger
        };
    }
    if units & NSWeekCalendarUnit != 0 {
        c.week = take(7.0 * 86400.0);
    }
    if units & NSDayCalendarUnit != 0 {
        c.day = take(86400.0);
    }
    if units & NSHourCalendarUnit != 0 {
        c.hour = take(3600.0);
    }
    if units & NSMinuteCalendarUnit != 0 {
        c.minute = take(60.0);
    }
    if units & NSSecondCalendarUnit != 0 {
        c.second = take(1.0);
    }
    autorelease(env, components)
}

@end

@implementation NSDateComponents: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSDateComponentsHostObject {
        era: NSUndefinedDateComponent,
        year: NSUndefinedDateComponent,
        month: NSUndefinedDateComponent,
        day: NSUndefinedDateComponent,
        hour: NSUndefinedDateComponent,
        minute: NSUndefinedDateComponent,
        second: NSUndefinedDateComponent,
        week: NSUndefinedDateComponent,
        weekday: NSUndefinedDateComponent,
        weekday_ordinal: NSUndefinedDateComponent,
    };
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

- (id)copyWithZone:(NSZonePtr)_zone {
    let copy: id = msg_class![env; NSDateComponents new];
    let c = *env.objc.borrow::<NSDateComponentsHostObject>(this);
    *env.objc.borrow_mut::<NSDateComponentsHostObject>(copy) = c;
    copy
}

- (NSInteger)era {
    env.objc.borrow::<NSDateComponentsHostObject>(this).era
}
- (())setEra:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).era = value;
}
- (NSInteger)year {
    env.objc.borrow::<NSDateComponentsHostObject>(this).year
}
- (())setYear:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).year = value;
}
- (NSInteger)month {
    env.objc.borrow::<NSDateComponentsHostObject>(this).month
}
- (())setMonth:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).month = value;
}
- (NSInteger)day {
    env.objc.borrow::<NSDateComponentsHostObject>(this).day
}
- (())setDay:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).day = value;
}
- (NSInteger)hour {
    env.objc.borrow::<NSDateComponentsHostObject>(this).hour
}
- (())setHour:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).hour = value;
}
- (NSInteger)minute {
    env.objc.borrow::<NSDateComponentsHostObject>(this).minute
}
- (())setMinute:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).minute = value;
}
- (NSInteger)second {
    env.objc.borrow::<NSDateComponentsHostObject>(this).second
}
- (())setSecond:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).second = value;
}
- (NSInteger)week {
    env.objc.borrow::<NSDateComponentsHostObject>(this).week
}
- (())setWeek:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).week = value;
}
- (NSInteger)weekday {
    env.objc.borrow::<NSDateComponentsHostObject>(this).weekday
}
- (())setWeekday:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).weekday = value;
}
- (NSInteger)weekdayOrdinal {
    env.objc.borrow::<NSDateComponentsHostObject>(this).weekday_ordinal
}
- (())setWeekdayOrdinal:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).weekday_ordinal = value;
}

@end

};

/// A date as local seconds since the reference date, in the calendar's time
/// zone.
fn local_time(env: &mut Environment, calendar: id, date: id) -> NSTimeInterval {
    let time_zone = env.objc.borrow::<NSCalendarHostObject>(calendar).time_zone;
    let offset: NSInteger = msg![env; time_zone secondsFromGMT];
    let interval: NSTimeInterval = msg![env; date timeIntervalSinceReferenceDate];
    interval + offset as NSTimeInterval
}

/// The inverse of [local_time]: an autoreleased `NSDate`.
fn new_date(env: &mut Environment, calendar: id, local: NSTimeInterval) -> id {
    let time_zone = env.objc.borrow::<NSCalendarHostObject>(calendar).time_zone;
    let offset: NSInteger = msg![env; time_zone secondsFromGMT];
    let interval = local - offset as NSTimeInterval;
    msg_class![env; NSDate dateWithTimeIntervalSinceReferenceDate:interval]
}

#[cfg(test)]
#[test]
fn test_civil_days() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(
        days_from_civil(2001, 1, 1),
        REFERENCE_DATE_FROM_UNIX / 86400
    );
    // Leap years, and after 2038.
    for days in [-800_000, -1, 0, 59, 60, 11_016, 25_000, 800_000] {
        let (y, m, d) = civil_from_days(days);
        assert_eq!(days_from_civil(y, m, d), days);
    }
    assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(2100, 2), 28);
    // 2026-11-03 is a Tuesday.
    let fields = to_fields(from_fields(2026, 11, 3, 0.0));
    assert_eq!(
        (fields.year, fields.month, fields.day, fields.weekday),
        (2026, 11, 3, 3)
    );
}
