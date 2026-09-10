/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CAAnimation` and its subclasses

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::core_animation::ca_media_timing_function::kCAMediaTimingFunctionDefault;
use crate::frameworks::core_foundation::time::CFTimeInterval;
use crate::frameworks::core_graphics::cg_path::{CGMutablePathRef, CGPathRelease, CGPathRetain};
use crate::frameworks::foundation::ns_string::{get_static_str, to_rust_string};
use crate::frameworks::foundation::NSUInteger;
use crate::objc::msg_send;
use crate::objc::{
    autorelease, id, msg, nil, objc_classes, release, retain, Class, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;
use crate::{impl_HostObject_with_superclass, msg_class, msg_super};
use std::collections::HashMap;

/// The layer key transitions are added under.
const kCATransition: &str = "transition";

pub type CATransitionType = id; // NSString*
pub const kCATransitionFade: &str = "fade";
pub const kCATransitionMoveIn: &str = "moveIn";
pub const kCATransitionPush: &str = "push";
pub const kCATransitionReveal: &str = "reveal";

pub type CATransitionSubtype = id; // NSString*
pub const kCATransitionFromRight: &str = "fromRight";
pub const kCATransitionFromLeft: &str = "fromLeft";
pub const kCATransitionFromTop: &str = "fromTop";
pub const kCATransitionFromBottom: &str = "fromBottom";

pub type CAMediaTimingFillMode = id; // NSString*
pub const kCAFillModeBackwards: &str = "backwards";
pub const kCAFillModeBoth: &str = "both";
pub const kCAFillModeForwards: &str = "forwards";
pub const kCAFillModeRemoved: &str = "removed";

pub type CAAnimationCalculationMode = id; // NSString*
pub const kCAAnimationLinear: &str = "linear";
pub const kCAAnimationDiscrete: &str = "discrete";
pub const kCAAnimationPaced: &str = "paced";
pub const kCAAnimationCubic: &str = "cubic";
pub const kCAAnimationCubicPaced: &str = "cubicPaced";

pub type CAAnimationRotationMode = id; // NSString*
const kCAAnimationRotateAuto: &str = "auto";
const kCAAnimationRotateAutoReverse: &str = "autoReverse";

pub const CONSTANTS: ConstantExports = &[
    // `CAAnimationCalculationMode` values.
    (
        "_kCAAnimationLinear",
        HostConstant::NSString(kCAAnimationLinear),
    ),
    (
        "_kCAAnimationDiscrete",
        HostConstant::NSString(kCAAnimationDiscrete),
    ),
    (
        "_kCAAnimationPaced",
        HostConstant::NSString(kCAAnimationPaced),
    ),
    (
        "_kCAAnimationCubic",
        HostConstant::NSString(kCAAnimationCubic),
    ),
    (
        "_kCAAnimationCubicPaced",
        HostConstant::NSString(kCAAnimationCubicPaced),
    ),
    // `CAAnimationRotationMode` values.
    (
        "_kCAAnimationRotateAuto",
        HostConstant::NSString(kCAAnimationRotateAuto),
    ),
    (
        "_kCAAnimationRotateAutoReverse",
        HostConstant::NSString(kCAAnimationRotateAutoReverse),
    ),
    ("_kCATransition", HostConstant::NSString(kCATransition)),
    // `CATransitionType` values.
    (
        "_kCATransitionFade",
        HostConstant::NSString(kCATransitionFade),
    ),
    (
        "_kCATransitionMoveIn",
        HostConstant::NSString(kCATransitionMoveIn),
    ),
    (
        "_kCATransitionPush",
        HostConstant::NSString(kCATransitionPush),
    ),
    (
        "_kCATransitionReveal",
        HostConstant::NSString(kCATransitionReveal),
    ),
    // `CATransitionSubtype` values.
    (
        "_kCATransitionFromRight",
        HostConstant::NSString(kCATransitionFromRight),
    ),
    (
        "_kCATransitionFromLeft",
        HostConstant::NSString(kCATransitionFromLeft),
    ),
    (
        "_kCATransitionFromTop",
        HostConstant::NSString(kCATransitionFromTop),
    ),
    (
        "_kCATransitionFromBottom",
        HostConstant::NSString(kCATransitionFromBottom),
    ),
    // `CAMediaTimingFillMode` values.
    (
        "_kCAFillModeBackwards",
        HostConstant::NSString(kCAFillModeBackwards),
    ),
    ("_kCAFillModeBoth", HostConstant::NSString(kCAFillModeBoth)),
    (
        "_kCAFillModeForwards",
        HostConstant::NSString(kCAFillModeForwards),
    ),
    (
        "_kCAFillModeRemoved",
        HostConstant::NSString(kCAFillModeRemoved),
    ),
];

struct CAAnimationHostObject {
    removed_on_completion: bool,
    timing_function: id, // CAMediaTimingFunction*
    delegate: id,        // CAAnimationDelegate*
    autoreverses: bool,
    repeat_count: f32,
    begin_time: CFTimeInterval,
    duration: CFTimeInterval,
    fill_mode: &'static str,
    started_at: Option<CFTimeInterval>,
    /// Whether the delegate has been told the animation stopped.
    stopped: bool,
    /// Values for keys that aren't properties (see `valueForKey:`), retained.
    other_values: HashMap<String, id>,
}
impl HostObject for CAAnimationHostObject {}
impl Default for CAAnimationHostObject {
    fn default() -> Self {
        Self {
            removed_on_completion: true,
            timing_function: Default::default(),
            delegate: Default::default(),
            autoreverses: Default::default(),
            repeat_count: Default::default(),
            begin_time: Default::default(),
            duration: Default::default(),
            fill_mode: kCAFillModeRemoved,
            started_at: None,
            stopped: false,
            other_values: HashMap::new(),
        }
    }
}

#[derive(Default)]
struct CAPropertyAnimationHostObject {
    superclass: CAAnimationHostObject,
    key_path: id, // NSString*
    cumulative: bool,
}
impl_HostObject_with_superclass!(CAPropertyAnimationHostObject);

#[derive(Default)]
struct CABasicAnimationHostObject {
    superclass: CAPropertyAnimationHostObject,
    from_value: id,
    to_value: id,
    by_value: id,
}
impl_HostObject_with_superclass!(CABasicAnimationHostObject);

struct CAKeyframeAnimationHostObject {
    superclass: CAPropertyAnimationHostObject,
    values: id,           // NSArray*
    key_times: id,        // NSArray<NSNumber*>*
    timing_functions: id, // NSArray<CAMediaTimingFunction*>*
    path: CGMutablePathRef,
    calculation_mode: &'static str,
    rotation_mode: id, // NSString*
}
impl_HostObject_with_superclass!(CAKeyframeAnimationHostObject);
impl Default for CAKeyframeAnimationHostObject {
    fn default() -> Self {
        Self {
            superclass: Default::default(),
            values: nil,
            key_times: nil,
            timing_functions: nil,
            path: nil,
            calculation_mode: kCAAnimationLinear,
            rotation_mode: nil,
        }
    }
}

struct CATransitionHostObject {
    superclass: CAAnimationHostObject,
    start_progress: f32,
    end_progress: f32,
    transition_type: CATransitionType,
    transition_subtype: CATransitionSubtype,
}
impl_HostObject_with_superclass!(CATransitionHostObject);
impl Default for CATransitionHostObject {
    fn default() -> Self {
        CATransitionHostObject {
            superclass: Default::default(),
            start_progress: 0.0,
            end_progress: 1.0,
            transition_type: nil,
            transition_subtype: nil,
        }
    }
}

#[derive(Default)]
struct CAAnimationGroupHostObject {
    superclass: CAAnimationHostObject,
    animations: id, // NSArray<CAAnimation*>*
}
impl_HostObject_with_superclass!(CAAnimationGroupHostObject);

/// The animations of a group, or [None] if it's not a group.
pub fn get_group_animations(env: &mut Environment, animation: id) -> Option<Vec<id>> {
    let class = env.objc.get_known_class("CAAnimationGroup", &mut env.mem);
    let animation_class: Class = msg![env; animation class];
    if !env.objc.class_is_subclass_of(animation_class, class) {
        return None;
    }
    let animations = env
        .objc
        .borrow::<CAAnimationGroupHostObject>(animation)
        .animations;
    Some(array_to_vec(env, animations))
}

/// What a keyframe animation animates between, for [super::animation].
pub struct Keyframes {
    /// `id`s of the values, if there's no path.
    pub values: Vec<id>,
    /// The path, which replaces `values` when set.
    pub path: CGMutablePathRef,
    pub key_times: Option<Vec<f32>>,
    /// `CAMediaTimingFunction*`s, one per segment.
    pub timing_functions: Vec<id>,
    pub calculation_mode: &'static str,
}

/// The keyframes of an animation, or [None] if it's not a keyframe one.
pub fn get_keyframes(env: &mut Environment, animation: id) -> Option<Keyframes> {
    let class = env
        .objc
        .get_known_class("CAKeyframeAnimation", &mut env.mem);
    let animation_class: Class = msg![env; animation class];
    if !env.objc.class_is_subclass_of(animation_class, class) {
        return None;
    }
    let &CAKeyframeAnimationHostObject {
        values,
        key_times,
        timing_functions,
        path,
        calculation_mode,
        ..
    } = env.objc.borrow(animation);
    let values = array_to_vec(env, values);
    let key_times = (key_times != nil).then(|| {
        array_to_vec(env, key_times)
            .into_iter()
            .map(|time| msg![env; time floatValue])
            .collect()
    });
    let timing_functions = array_to_vec(env, timing_functions);
    Some(Keyframes {
        values,
        path,
        key_times,
        timing_functions,
        calculation_mode,
    })
}

fn array_to_vec(env: &mut Environment, array: id) -> Vec<id> {
    if array == nil {
        return Vec::new();
    }
    let count: NSUInteger = msg![env; array count];
    (0..count)
        .map(|i| msg![env; array objectAtIndex:i])
        .collect()
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CAAnimation is an abstract class.
@implementation CAAnimation: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CAAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)animation {
    let object = msg![env; this new];
    autorelease(env, object)
}

- (id)init {
    let default_timing_function_name: id = get_static_str(env, kCAMediaTimingFunctionDefault);
    let default_timing_function: id = msg_class![env; CAMediaTimingFunction functionWithName: default_timing_function_name];
    () = msg![env; this setTimingFunction: default_timing_function];
    this
}

- (())setRemovedOnCompletion:(bool)removed_on_completion {
    log_dbg!("[(CAAnimation*){:?} setRemovedOnCompletion:{:?}]", this, removed_on_completion);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).removed_on_completion = removed_on_completion;
}
- (bool)isRemovedOnCompletion {
    env.objc.borrow::<CAAnimationHostObject>(this).removed_on_completion
}

- (())setDelegate:(id)delegate { // CAAnimationDelegate*
    log_dbg!("[(CAAnimation*){:?} setDelegate:{:?}]", this, delegate);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).delegate = delegate;
    retain(env, delegate);
}
- (id)delegate {
    env.objc.borrow::<CAAnimationHostObject>(this).delegate
}

- (())setTimingFunction:(id)timingFunction { // CAMediaTimingFunction*
    log_dbg!("[(CAAnimation*){:?} setTimingFunction:{:?}]", this, timingFunction);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).timing_function = timingFunction;
    retain(env, timingFunction);
}
- (id)timingFunction {
    env.objc.borrow::<CAAnimationHostObject>(this).timing_function
}

// CAMediaTiming protocol implementation
- (())setAutoreverses:(bool)autoreverses {
    log_dbg!("[(CAAnimation*){:?} setAutoreverses:{:?}]", this, autoreverses);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).autoreverses = autoreverses;
}
- (bool)autoreverses {
    env.objc.borrow::<CAAnimationHostObject>(this).autoreverses
}

- (())setRepeatCount:(f32)repeatCount {
    log_dbg!("[(CAAnimation*){:?} setRepeatCount:{:?}]", this, repeatCount);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).repeat_count = repeatCount;
}
- (f32)repeatCount {
    env.objc.borrow::<CAAnimationHostObject>(this).repeat_count
}

- (())setBeginTime:(CFTimeInterval)beginTime {
    log_dbg!("[(CAAnimation*){:?} setBeginTime:{:?}]", this, beginTime);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).begin_time = beginTime;
}
- (CFTimeInterval)beginTime {
    env.objc.borrow::<CAAnimationHostObject>(this).begin_time
}

- (())setDuration:(CFTimeInterval)duration {
    log_dbg!("[(CAAnimation*){:?} setDuration:{:?}]", this, duration);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).duration = duration;
}
- (CFTimeInterval)duration {
    env.objc.borrow::<CAAnimationHostObject>(this).duration
}

- (())setFillMode:(CAMediaTimingFillMode)fill_mode {
    let fill_mode_str = to_rust_string(env, fill_mode);
    log_dbg!("[(CAAnimation*){:?} setFillMode:{:?} ({})]", this, fill_mode, fill_mode_str);
    let fill_mode_str = match &*fill_mode_str {
        kCAFillModeBackwards => kCAFillModeBackwards,
        kCAFillModeBoth => kCAFillModeBoth,
        kCAFillModeForwards => kCAFillModeForwards ,
        kCAFillModeRemoved => kCAFillModeRemoved ,
        _ => panic!("Unknown fill mode \"{}\"", fill_mode_str)
    };
    env.objc.borrow_mut::<CAAnimationHostObject>(this).fill_mode = fill_mode_str;
}
- (CAMediaTimingFillMode)fillMode {
    let fill_mode = env.objc.borrow::<CAAnimationHostObject>(this).fill_mode;
    get_static_str(env, fill_mode)
}

// Like CALayer, CAAnimation keeps values for any key that isn't a property.
- (())setValue:(id)value forUndefinedKey:(id)key { // NSString*
    let key = to_rust_string(env, key).into_owned();
    retain(env, value);
    let host_obj = env.objc.borrow_mut::<CAAnimationHostObject>(this);
    let old = if value == nil {
        host_obj.other_values.remove(&key)
    } else {
        host_obj.other_values.insert(key, value)
    };
    if let Some(old) = old {
        release(env, old);
    }
}
- (id)valueForKey:(id)key { // NSString*
    let key_str = to_rust_string(env, key).into_owned();
    if let Some(&value) = env.objc.borrow::<CAAnimationHostObject>(this).other_values.get(&key_str) {
        return value;
    }
    match &*key_str {
        "duration" | "beginTime" => {
            let selector = env.objc.lookup_selector(&key_str).unwrap();
            let value: CFTimeInterval = msg_send(env, (this, selector));
            msg_class![env; NSNumber numberWithDouble:value]
        }
        "repeatCount" => {
            let value: f32 = msg![env; this repeatCount];
            msg_class![env; NSNumber numberWithFloat:value]
        }
        "autoreverses" | "removedOnCompletion" => {
            let selector = if key_str == "autoreverses" { "autoreverses" } else { "isRemovedOnCompletion" };
            let selector = env.objc.lookup_selector(selector).unwrap();
            let value: bool = msg_send(env, (this, selector));
            msg_class![env; NSNumber numberWithBool:value]
        }
        // Properties holding objects.
        "delegate" | "timingFunction" | "fillMode" | "keyPath" | "fromValue" | "toValue"
        | "byValue" | "values" | "keyTimes" | "timingFunctions" | "calculationMode"
        | "rotationMode" | "animations" => {
            let selector = env.objc.lookup_selector(&key_str).unwrap();
            msg_send(env, (this, selector))
        }
        // An unset key: nil, like on a device.
        _ => nil,
    }
}

// NSCopying implementation. The copy hasn't started or stopped yet.
- (id)copyWithZone:(NSZonePtr)_zone {
    let class: Class = msg![env; this class];
    let new: id = msg![env; class alloc];
    () = msg![env; new _touchHLE_copyPropertiesFrom:this];
    new
}

// Each class copies its own properties, then calls its superclass.
- (())_touchHLE_copyPropertiesFrom:(id)other { // CAAnimation*
    let other_host = env.objc.borrow::<CAAnimationHostObject>(other);
    let other_values = other_host.other_values.clone();
    let &CAAnimationHostObject {
        removed_on_completion,
        timing_function,
        delegate,
        autoreverses,
        repeat_count,
        begin_time,
        duration,
        fill_mode,
        ..
    } = other_host;
    retain(env, timing_function);
    retain(env, delegate);
    for &value in other_values.values() {
        retain(env, value);
    }
    let host = env.objc.borrow_mut::<CAAnimationHostObject>(this);
    host.removed_on_completion = removed_on_completion;
    host.timing_function = timing_function;
    host.delegate = delegate;
    host.autoreverses = autoreverses;
    host.repeat_count = repeat_count;
    host.begin_time = begin_time;
    host.duration = duration;
    host.fill_mode = fill_mode;
    host.other_values = other_values;
}

- (())dealloc {
    let &CAAnimationHostObject { delegate, timing_function, .. } = env.objc.borrow(this);
    if delegate != nil {
        release(env, delegate);
    }
    if timing_function != nil {
        release(env, timing_function);
    }
    let other_values = std::mem::take(&mut env.objc.borrow_mut::<CAAnimationHostObject>(this).other_values);
    for value in other_values.into_values() {
        release(env, value);
    }

    env.objc.dealloc_object(this, &mut env.mem)
}

@end


@implementation CAPropertyAnimation: CAAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CAPropertyAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)animationWithKeyPath:(id)path { // NSString*
    let object = msg![env; this new];
    log_dbg!("[CAPropertyAnimation animationWithKeyPath:{:?} ({:?})] -> {:?}", path, to_rust_string(env, path), object);
    () = msg![env; object setKeyPath:path];
    autorelease(env, object)
}

- (())setKeyPath:(id)path { // NSString*
    log_dbg!("[(CAPropertyAnimation*){:?} setKeyPath:{:?} ({:?})]", this, path, to_rust_string(env, path));
    let path_copy: id = msg![env; path copy];
    env.objc.borrow_mut::<CAPropertyAnimationHostObject>(this).key_path = path_copy;
}
- (id)keyPath {
    env.objc.borrow::<CAPropertyAnimationHostObject>(this).key_path
}

- (())setCumulative:(bool)cumulative {
    env.objc.borrow_mut::<CAPropertyAnimationHostObject>(this).cumulative = cumulative;
}
- (bool)isCumulative {
    env.objc.borrow::<CAPropertyAnimationHostObject>(this).cumulative
}

- (())_touchHLE_copyPropertiesFrom:(id)other { // CAPropertyAnimation*
    () = msg_super![env; this _touchHLE_copyPropertiesFrom:other];
    let &CAPropertyAnimationHostObject {
        key_path,
        cumulative,
        ..
    } = env.objc.borrow(other);
    retain(env, key_path);
    let host = env.objc.borrow_mut::<CAPropertyAnimationHostObject>(this);
    host.key_path = key_path;
    host.cumulative = cumulative;
}

- (())dealloc {
    let &CAPropertyAnimationHostObject { key_path, .. } = env.objc.borrow(this);
    if key_path != nil {
        release(env, key_path);
    }

    msg_super![env; this dealloc]
}

@end


@implementation CABasicAnimation: CAPropertyAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CABasicAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())setFromValue:(id)value {
    log_dbg!("[(CABasicAnimation*){:?} setFromValue:{:?}]", this, value);
    env.objc.borrow_mut::<CABasicAnimationHostObject>(this).from_value = value;
    retain(env, value);
}
- (id)fromValue {
    env.objc.borrow::<CABasicAnimationHostObject>(this).from_value
}

- (())setToValue:(id)value {
    log_dbg!("[(CABasicAnimation*){:?} setToValue:{:?}]", this, value);
    env.objc.borrow_mut::<CABasicAnimationHostObject>(this).to_value = value;
    retain(env, value);
}
- (id)toValue {
    env.objc.borrow::<CABasicAnimationHostObject>(this).to_value
}

- (())setByValue:(id)value {
    log_dbg!("[(CABasicAnimation*){:?} setByValue:{:?}]", this, value);
    env.objc.borrow_mut::<CABasicAnimationHostObject>(this).by_value = value;
    retain(env, value);
}
- (id)byValue {
    env.objc.borrow::<CABasicAnimationHostObject>(this).by_value
}

- (())_touchHLE_copyPropertiesFrom:(id)other { // CABasicAnimation*
    () = msg_super![env; this _touchHLE_copyPropertiesFrom:other];
    let &CABasicAnimationHostObject {
        from_value,
        to_value,
        by_value,
        ..
    } = env.objc.borrow(other);
    retain(env, from_value);
    retain(env, to_value);
    retain(env, by_value);
    let host = env.objc.borrow_mut::<CABasicAnimationHostObject>(this);
    host.from_value = from_value;
    host.to_value = to_value;
    host.by_value = by_value;
}

- (())dealloc {
    let &CABasicAnimationHostObject { from_value, to_value, .. } = env.objc.borrow(this);
    if from_value != nil {
        release(env, from_value);
    }
    if to_value != nil {
        release(env, to_value);
    }

    msg_super![env; this dealloc]
}

@end


@implementation CAKeyframeAnimation: CAPropertyAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CAKeyframeAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    let this: id = msg_super![env; this init];
    // Unlike for UIView animations, no timing function means linear pacing.
    () = msg![env; this setTimingFunction:nil];
    this
}

- (())setValues:(id)values { // NSArray*
    let values: id = msg![env; values copy];
    let host_obj = env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this);
    let old = std::mem::replace(&mut host_obj.values, values);
    release(env, old);
}
- (id)values {
    env.objc.borrow::<CAKeyframeAnimationHostObject>(this).values
}

- (())setKeyTimes:(id)key_times { // NSArray<NSNumber*>*
    let key_times: id = msg![env; key_times copy];
    let host_obj = env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this);
    let old = std::mem::replace(&mut host_obj.key_times, key_times);
    release(env, old);
}
- (id)keyTimes {
    env.objc.borrow::<CAKeyframeAnimationHostObject>(this).key_times
}

- (())setTimingFunctions:(id)functions { // NSArray<CAMediaTimingFunction*>*
    let functions: id = msg![env; functions copy];
    let host_obj = env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this);
    let old = std::mem::replace(&mut host_obj.timing_functions, functions);
    release(env, old);
}
- (id)timingFunctions {
    env.objc.borrow::<CAKeyframeAnimationHostObject>(this).timing_functions
}

- (())setPath:(CGMutablePathRef)path {
    CGPathRetain(env, path);
    let host_obj = env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this);
    let old = std::mem::replace(&mut host_obj.path, path);
    CGPathRelease(env, old);
}
- (CGMutablePathRef)path {
    env.objc.borrow::<CAKeyframeAnimationHostObject>(this).path
}

- (())setCalculationMode:(CAAnimationCalculationMode)mode {
    let mode_str = to_rust_string(env, mode);
    let mode_str = match &*mode_str {
        kCAAnimationLinear => kCAAnimationLinear,
        kCAAnimationDiscrete => kCAAnimationDiscrete,
        kCAAnimationPaced => kCAAnimationPaced,
        kCAAnimationCubic => kCAAnimationCubic,
        kCAAnimationCubicPaced => kCAAnimationCubicPaced,
        _ => panic!("Unknown calculation mode \"{}\"", mode_str),
    };
    env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this).calculation_mode = mode_str;
}
- (CAAnimationCalculationMode)calculationMode {
    let mode = env.objc.borrow::<CAKeyframeAnimationHostObject>(this).calculation_mode;
    get_static_str(env, mode)
}

- (())setRotationMode:(CAAnimationRotationMode)mode {
    if mode != nil {
        log!("TODO: CAKeyframeAnimation rotationMode {:?} is not applied", to_rust_string(env, mode));
    }
    let mode: id = msg![env; mode copy];
    let host_obj = env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this);
    let old = std::mem::replace(&mut host_obj.rotation_mode, mode);
    release(env, old);
}
- (CAAnimationRotationMode)rotationMode {
    env.objc.borrow::<CAKeyframeAnimationHostObject>(this).rotation_mode
}

- (())_touchHLE_copyPropertiesFrom:(id)other { // CAKeyframeAnimation*
    () = msg_super![env; this _touchHLE_copyPropertiesFrom:other];
    let &CAKeyframeAnimationHostObject {
        values,
        key_times,
        timing_functions,
        path,
        calculation_mode,
        rotation_mode,
        ..
    } = env.objc.borrow(other);
    retain(env, values);
    retain(env, key_times);
    retain(env, timing_functions);
    CGPathRetain(env, path);
    retain(env, rotation_mode);
    let host = env.objc.borrow_mut::<CAKeyframeAnimationHostObject>(this);
    host.values = values;
    host.key_times = key_times;
    host.timing_functions = timing_functions;
    host.path = path;
    host.calculation_mode = calculation_mode;
    host.rotation_mode = rotation_mode;
}

- (())dealloc {
    let &CAKeyframeAnimationHostObject {
        values,
        key_times,
        timing_functions,
        path,
        rotation_mode,
        ..
    } = env.objc.borrow(this);
    release(env, values);
    release(env, key_times);
    release(env, timing_functions);
    CGPathRelease(env, path);
    release(env, rotation_mode);
    msg_super![env; this dealloc]
}

@end


@implementation CAAnimationGroup: CAAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CAAnimationGroupHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    let this: id = msg_super![env; this init];
    // No timing function means linear pacing.
    () = msg![env; this setTimingFunction:nil];
    this
}

- (())setAnimations:(id)animations { // NSArray<CAAnimation*>*
    let animations: id = msg![env; animations copy];
    let host_obj = env.objc.borrow_mut::<CAAnimationGroupHostObject>(this);
    let old = std::mem::replace(&mut host_obj.animations, animations);
    release(env, old);
}
- (id)animations {
    env.objc.borrow::<CAAnimationGroupHostObject>(this).animations
}

- (())_touchHLE_copyPropertiesFrom:(id)other { // CAAnimationGroup*
    () = msg_super![env; this _touchHLE_copyPropertiesFrom:other];
    let animations = env.objc.borrow::<CAAnimationGroupHostObject>(other).animations;
    retain(env, animations);
    env.objc.borrow_mut::<CAAnimationGroupHostObject>(this).animations = animations;
}

- (())dealloc {
    let animations = env.objc.borrow::<CAAnimationGroupHostObject>(this).animations;
    release(env, animations);
    msg_super![env; this dealloc]
}

@end


@implementation CATransition : CAAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::<CATransitionHostObject>::default(), &mut env.mem)
}

- (id)init {
    () = msg![env; this setType:(get_static_str(env, kCATransitionFade))];
    msg_super![env; this init]
}

- (())setStartProgress:(f32)start_progress {
    env.objc.borrow_mut::<CATransitionHostObject>(this).start_progress = start_progress;
}
- (f32)startProgress {
    env.objc.borrow::<CATransitionHostObject>(this).start_progress
}

- (())setEndProgress:(f32)end_progress {
    env.objc.borrow_mut::<CATransitionHostObject>(this).end_progress = end_progress;
}
- (f32)endProgress {
    env.objc.borrow::<CATransitionHostObject>(this).end_progress
}

- (())setType:(CATransitionType)transition_type {
    assert!(transition_type != nil);
    env.objc.borrow_mut::<CATransitionHostObject>(this).transition_type = msg![env; transition_type copy];
}
- (CATransitionType)type {
    env.objc.borrow::<CATransitionHostObject>(this).transition_type
}

- (())setSubtype:(CATransitionSubtype)transition_subtype {
    env.objc.borrow_mut::<CATransitionHostObject>(this).transition_subtype = msg![env; transition_subtype copy];
}
- (CATransitionSubtype)subtype {
    env.objc.borrow::<CATransitionHostObject>(this).transition_subtype
}

- (())dealloc {
    let &CATransitionHostObject { transition_type, .. } = env.objc.borrow(this);
    release(env, transition_type);
    msg_super![env; this dealloc]
}

@end

};

pub fn get_animation_start_time(
    env: &mut Environment,
    animation: id,
) -> &mut Option<CFTimeInterval> {
    &mut env
        .objc
        .borrow_mut::<CAAnimationHostObject>(animation)
        .started_at
}

pub fn get_animation_stopped(env: &mut Environment, animation: id) -> &mut bool {
    &mut env
        .objc
        .borrow_mut::<CAAnimationHostObject>(animation)
        .stopped
}
