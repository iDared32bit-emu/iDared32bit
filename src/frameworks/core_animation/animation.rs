/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Functions, traits, and all kinds of things to assist with bridging the gap
//! between guest and host when it comes to animations in Core Animation.
//! Based in Apple's documented behavior for Core Animation, although not an
//! exact match.
//! References:
//! - Core Animation Programming Guide
//!   <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/Introduction/Introduction.html>
//! - List of Animatable properties
//!   <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/AnimatableProperties/AnimatableProperties.html#//apple_ref/doc/uid/TP40004514-CH11-SW2>
//! - Animation timing behavior, layers' local time, autoreverses, etc.
//!   <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/AdvancedAnimationTricks/AdvancedAnimationTricks.html>
//! - Algorithm for choosing interpolation values
//!   <https://developer.apple.com/documentation/quartzcore/cabasicanimation?language=objc>
use std::ops::Sub;

use crate::frameworks::core_animation::ca_animation::{
    get_animation_start_time, get_animation_stopped, get_group_animations, get_keyframes,
    get_transition, kCAAnimationCubic, kCAAnimationCubicPaced, kCAAnimationDiscrete,
    kCAAnimationPaced, kCAFillModeBackwards, kCAFillModeBoth, kCAFillModeForwards,
    kCATransitionFade, CAMediaTimingFillMode, Keyframes,
};
use crate::frameworks::core_animation::ca_layer::remove_anonymous_animation;
use crate::frameworks::core_animation::ca_transform_3d::CATransform3D;
use crate::frameworks::core_animation::{ca_layer::CALayerHostObject, CACurrentMediaTime};
use crate::frameworks::core_foundation::time::CFTimeInterval;
use crate::frameworks::core_graphics::cg_affine_transform::CGAffineTransform;
use crate::frameworks::core_graphics::cg_color::CGColorHostObject;
use crate::frameworks::core_graphics::cg_path::path_points;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{from_rust_string, to_rust_string};
use crate::objc::{id, msg, nil, release, retain};
use crate::Environment;

#[derive(Default)]
pub struct State {
    started_animations: Vec<id>,
    finished_animations: Vec<(id, id, bool, bool, Option<String>)>,
}
impl State {
    pub fn create_presentation_layer(
        &mut self,
        env: &mut Environment,
        layer: id,
    ) -> CALayerHostObject {
        // Clone given layer
        let original = env.objc.borrow::<CALayerHostObject>(layer);
        let mut presentation = original.clone();

        // Loop over all animations and set the presentation layer's values
        let named_animations: Vec<(Option<String>, id)> = presentation
            .animations
            .iter()
            .map(|(key, anim)| (Some(key.clone()), *anim))
            .collect();
        let anonymous_animations: Vec<(Option<String>, id)> = presentation
            .anonymous_animations
            .iter()
            .map(|anim| (None, *anim))
            .collect();

        for (key, animation) in
            Iterator::chain(named_animations.iter(), anonymous_animations.iter())
        {
            let animation = *animation;

            let fill_mode: CAMediaTimingFillMode = msg![env; animation fillMode];
            let fill_mode = to_rust_string(env, fill_mode);

            // TODO: Convert to local time
            let current_time = CACurrentMediaTime(env);
            let begin_time: CFTimeInterval = msg![env; animation beginTime];
            let start_time = get_animation_start_time(env, animation);

            if current_time >= begin_time {
                if start_time.is_none() {
                    // Animation started but isn't marked as such
                    start_time.replace(current_time);
                    self.started_animations.push(animation);
                }
            } else if fill_mode != kCAFillModeBackwards && fill_mode != kCAFillModeBoth {
                continue;
            }

            let effective_begin_time = start_time.unwrap_or(begin_time);

            if let Some(key) = key {
                log_dbg!(
                    "Animate CALayer {:?} animation {} {:?}",
                    layer,
                    key,
                    animation
                );
            } else {
                log_dbg!("Animate CALayer {:?} animation {:?}", layer, animation);
            }

            let repeat_count: f32 = msg![env; animation repeatCount];
            assert!(repeat_count >= 0.0);
            // Setting [repeatCount] to greatestFiniteMagnitude will cause
            // the animation to repeat forever.
            let effective_repeat_count = if repeat_count == f32::MAX {
                f32::INFINITY
            } else if repeat_count == 0.0 {
                1.0
            } else {
                repeat_count
            };

            let duration: CFTimeInterval = msg![env; animation duration];
            let current_repeat = (((current_time - effective_begin_time).max(0.0) / duration)
                as f32)
                .min(effective_repeat_count);

            let autoreverses: bool = msg![env; animation autoreverses];
            let progress = repeat_progress(current_repeat, effective_repeat_count, autoreverses);

            if current_repeat >= effective_repeat_count {
                // Animations kept after completion only stop once.
                let stopped = get_animation_stopped(env, animation);
                if !*stopped {
                    *stopped = true;
                    let removed_on_completion: bool = msg![env; animation isRemovedOnCompletion];
                    self.finished_animations.push((
                        layer,
                        animation,
                        true,
                        removed_on_completion,
                        key.to_owned(),
                    ));
                }
                if fill_mode != kCAFillModeForwards && fill_mode != kCAFillModeBoth {
                    continue;
                }
            }

            apply_animation(
                env,
                &mut presentation,
                animation,
                progress,
                current_repeat.floor(),
            );
        }

        presentation
    }

    pub fn update_started_and_finished_animations(self, env: &mut Environment) {
        // A delegate may remove animations, so keep them alive until done.
        for (layer, animation, ..) in &self.finished_animations {
            retain(env, *layer);
            retain(env, *animation);
        }
        for animation in self.started_animations {
            let delegate = msg![env; animation delegate];
            // The protocol marks animationDidStart: as optional
            if delegate != nil
                && env
                    .objc
                    .object_has_method_named(&env.mem, delegate, "animationDidStart:")
            {
                () = msg![env; delegate animationDidStart: animation];
            }
        }

        for (layer, animation, finished, removed_on_completion, key) in self.finished_animations {
            let delegate = msg![env; animation delegate];
            // The protocol marks animationDidStop:finished: as optional
            if delegate != nil
                && env.objc.object_has_method_named(
                    &env.mem,
                    delegate,
                    "animationDidStop:finished:",
                )
            {
                () = msg![env; delegate animationDidStop: animation finished: finished];
            }

            if removed_on_completion {
                if let Some(key) = key {
                    let key = from_rust_string(env, key);
                    () = msg![env; layer removeAnimationForKey: key];
                } else {
                    remove_anonymous_animation(env, layer, animation);
                }
            }

            release(env, layer);
            release(env, animation);
        }
    }
}

/// Set an animation's value at `progress` (0 to 1, before its timing
/// function) in the presentation layer. `repeats_done` is for cumulative
/// animations, which carry on from where the previous repeat ended.
fn apply_animation(
    env: &mut Environment,
    presentation: &mut CALayerHostObject,
    animation: id,
    progress: f32,
    repeats_done: f32,
) {
    let timing_function: id = msg![env; animation timingFunction];
    let amount = if timing_function == nil {
        progress
    } else {
        msg![env; timing_function _solveForInput: progress]
    };

    if let Some(children) = get_group_animations(env, animation) {
        let duration: CFTimeInterval = msg![env; animation duration];
        let elapsed = f64::from(amount) * duration;
        for child in children {
            apply_group_child(env, presentation, child, elapsed, duration);
        }
        return;
    }

    if let Some((transition_type, start, end)) = get_transition(env, animation) {
        if transition_type != kCATransitionFade {
            unimplemented!("CATransition type {:?} isn't implemented", transition_type);
        }
        // The compositor fades the old picture out over the new one.
        presentation.fade_transition = Some((animation, start + (end - start) * amount));
        return;
    }

    if let Some(keyframes) = get_keyframes(env, animation) {
        let key_path: id = msg![env; animation keyPath];
        let key_path = to_rust_string(env, key_path).into_owned();
        apply_keyframes(env, presentation, &key_path, keyframes, amount);
        return;
    }

    // Otherwise, it's a CABasicAnimation
    let mut interpolation_amount = amount;
    let cumulative: bool = msg![env; animation isCumulative];
    if cumulative {
        interpolation_amount += repeats_done;
    }

    let from_value: id = msg![env; animation fromValue];
    let to_value: id = msg![env; animation toValue];
    let by_value: id = msg![env; animation byValue];

    // Update values only in the presentation layer
    let key_path: id = msg![env; animation keyPath];
    let key_path = to_rust_string(env, key_path);
    // Only these properties are animatable
    // TODO: Implement for all properties
    match &*key_path {
        "anchorPoint" => {
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj CGPointValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj CGPointValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj CGPointValue]);
            let (from_value, by_value) = get_from_and_by_values(
                Some(presentation.anchor_point),
                from_value,
                to_value,
                by_value,
            );
            presentation.anchor_point = from_value + by_value * interpolation_amount;
        }
        "backgroundColor" => {
            let from_value =
                id_as_option(from_value).map(|obj| *env.objc.borrow::<CGColorHostObject>(obj));
            let to_value =
                id_as_option(to_value).map(|obj| *env.objc.borrow::<CGColorHostObject>(obj));
            let by_value =
                id_as_option(by_value).map(|obj| *env.objc.borrow::<CGColorHostObject>(obj));
            let (from_value, by_value) = get_from_and_by_values(
                presentation.background_color,
                from_value,
                to_value,
                by_value,
            );
            presentation.background_color = Some(from_value + by_value * interpolation_amount)
        }
        "bounds" => {
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj CGRectValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj CGRectValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj CGRectValue]);
            let (from_value, by_value) =
                get_from_and_by_values(Some(presentation.bounds), from_value, to_value, by_value);
            presentation.bounds = from_value + by_value * interpolation_amount;
        }
        "cornerRadius" => {
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj floatValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj floatValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj floatValue]);
            let (from_value, by_value) = get_from_and_by_values(
                Some(presentation.corner_radius),
                from_value,
                to_value,
                by_value,
            );
            presentation.corner_radius = from_value + by_value * interpolation_amount;
        }
        "hidden" => {
            let from_value = id_as_option(from_value)
                .map(|obj| msg![env; obj boolValue])
                .map(|val: bool| val as i32 as f32);
            let to_value = id_as_option(to_value)
                .map(|obj| msg![env; obj boolValue])
                .map(|val: bool| val as i32 as f32);
            let by_value = id_as_option(by_value)
                .map(|obj| msg![env; obj boolValue])
                .map(|val: bool| val as i32 as f32);
            let (from_value, by_value) = get_from_and_by_values(
                Some(presentation.hidden as i32 as f32),
                from_value,
                to_value,
                by_value,
            );
            presentation.hidden = (from_value + by_value * interpolation_amount) > 0.5;
        }
        "opacity" => {
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj floatValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj floatValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj floatValue]);
            let (from_value, by_value) =
                get_from_and_by_values(Some(presentation.opacity), from_value, to_value, by_value);
            presentation.opacity = from_value + by_value * interpolation_amount;
        }
        "position" => {
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj CGPointValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj CGPointValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj CGPointValue]);
            let (from_value, by_value) =
                get_from_and_by_values(Some(presentation.position), from_value, to_value, by_value);
            presentation.position = from_value + by_value * interpolation_amount;
        }
        "transform.rotation" | "transform.rotation.z" => {
            let current = presentation.affine_transform;
            let current_angle = current.b.atan2(current.a);
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj floatValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj floatValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj floatValue]);
            let (from_value, by_value) =
                get_from_and_by_values(Some(current_angle), from_value, to_value, by_value);
            let angle = from_value + by_value * interpolation_amount;
            presentation.affine_transform = current.rotate(angle - current_angle);
        }
        "transform.scale" | "transform.scale.x" | "transform.scale.y" => {
            // The x and y scales, as a point.
            let current = CGPoint {
                x: get_scale(presentation, "transform.scale.x"),
                y: get_scale(presentation, "transform.scale.y"),
            };
            let from_value = id_as_option(from_value).map(|obj| scale_value(env, obj));
            let to_value = id_as_option(to_value).map(|obj| scale_value(env, obj));
            let by_value = id_as_option(by_value).map(|obj| scale_value(env, obj));
            let (from_value, by_value) =
                get_from_and_by_values(Some(current), from_value, to_value, by_value);
            let scale = from_value + by_value * interpolation_amount;
            if key_path != "transform.scale.y" {
                set_scale(presentation, "transform.scale.x", scale.x);
            }
            if key_path != "transform.scale.x" {
                set_scale(presentation, "transform.scale.y", scale.y);
            }
        }
        "transform.translation.x" | "transform.translation.y" => {
            let from_value = id_as_option(from_value).map(|obj| msg![env; obj floatValue]);
            let to_value = id_as_option(to_value).map(|obj| msg![env; obj floatValue]);
            let by_value = id_as_option(by_value).map(|obj| msg![env; obj floatValue]);
            let is_x = key_path.ends_with('x');
            let transform = presentation.affine_transform;
            let current = if is_x { transform.tx } else { transform.ty };
            let (from_value, by_value) =
                get_from_and_by_values(Some(current), from_value, to_value, by_value);
            let value = from_value + by_value * interpolation_amount;
            if is_x {
                presentation.affine_transform.tx = value;
            } else {
                presentation.affine_transform.ty = value;
            }
        }
        "transform" => {
            assert!(by_value == nil, "TODO: byValue for transform animations");
            let current = transform_components(presentation.affine_transform);
            let mut value_of = |value: id| match id_as_option(value) {
                Some(value) => transform_value_components(env, value),
                None => current.clone(),
            };
            let (from_value, to_value) = (value_of(from_value), value_of(to_value));
            let to_value = unwrap_angle(&from_value, to_value);
            let value: Components = from_value
                .iter()
                .zip(&to_value)
                .map(|(a, b)| a + (b - a) * interpolation_amount)
                .collect();
            presentation.affine_transform = transform_from_components(&value);
        }
        _ => panic!("Attempted to animate on key {}", key_path),
    }
}

/// Progress through the current repeat (0 to 1), from the number of repeats
/// done so far (with a fraction) and the number to do.
fn repeat_progress(current_repeat: f32, repeat_count: f32, autoreverses: bool) -> f32 {
    // At the very end, show the end of the last repeat, not the start of
    // another one.
    let progress = if current_repeat >= repeat_count
        && current_repeat.fract() == 0.0
        && current_repeat > 0.0
    {
        1.0
    } else {
        current_repeat.fract()
    };
    if autoreverses {
        // From the docs:
        // Setting the repeat count to a whole number (such as 1.0) for
        // an autoreversing animation causes the animation to stop on
        // its starting value.
        // Adding an extra half step (such as a repeat count of 1.5)
        // causes the animation to stop on its end value
        ((progress * 2.0 - 1.0).abs() - 1.0).abs()
    } else {
        progress
    }
}

/// Apply an animation in a group, `elapsed` seconds into the group.
fn apply_group_child(
    env: &mut Environment,
    presentation: &mut CALayerHostObject,
    child: id,
    elapsed: CFTimeInterval,
    group_duration: CFTimeInterval,
) {
    let begin_time: CFTimeInterval = msg![env; child beginTime];
    let duration: CFTimeInterval = msg![env; child duration];
    // Children without their own duration take the group's.
    let duration = if duration > 0.0 {
        duration
    } else {
        group_duration
    };
    let repeat_count: f32 = msg![env; child repeatCount];
    let repeat_count = if repeat_count == 0.0 {
        1.0
    } else {
        repeat_count
    };
    let fill_mode: CAMediaTimingFillMode = msg![env; child fillMode];
    let fill_mode = to_rust_string(env, fill_mode);
    let fills_backwards = fill_mode == kCAFillModeBackwards || fill_mode == kCAFillModeBoth;
    let fills_forwards = fill_mode == kCAFillModeForwards || fill_mode == kCAFillModeBoth;

    let local = elapsed - begin_time;
    if local < 0.0 && !fills_backwards {
        return;
    }
    let current_repeat = ((local.max(0.0) / duration) as f32).min(repeat_count);
    if current_repeat >= repeat_count && !fills_forwards {
        return;
    }
    let autoreverses: bool = msg![env; child autoreverses];
    let progress = repeat_progress(current_repeat, repeat_count, autoreverses);
    apply_animation(env, presentation, child, progress, current_repeat.floor());
}

fn get_from_and_by_values<T>(
    current_value: Option<T>,
    from_value: Option<T>,
    to_value: Option<T>,
    by_value: Option<T>,
) -> (T, T)
where
    T: Copy + Sub<Output = T>,
{
    if from_value.is_some() && to_value.is_some() && by_value.is_some() {
        panic!("Cannot specify all three of fromValue, toValue, and byValue");
    } else if let (Some(from_value), Some(to_value)) = (from_value, to_value) {
        let by_value = to_value - from_value.to_owned();
        (from_value, by_value)
    } else if let (Some(from_value), Some(by_value)) = (from_value, by_value) {
        (from_value.to_owned(), by_value.to_owned())
    } else if let (Some(to_value), Some(by_value)) = (to_value, by_value) {
        let from_value = to_value - by_value;
        (from_value, by_value.to_owned())
    } else if let Some(from_value) = from_value {
        let by_value = current_value.unwrap() - from_value;
        (from_value.to_owned(), by_value)
    } else if let Some(to_value) = to_value {
        let from_value = current_value.unwrap();
        let by_value = to_value - from_value;
        (from_value.to_owned(), by_value)
    } else if let Some(by_value) = by_value {
        let from_value = current_value.unwrap();
        (from_value.to_owned(), by_value.to_owned())
    } else {
        // TODO: All properties are nil. Interpolates between the previous
        // value of keyPath in the target layer’s presentation layer and the
        // current value of keyPath in the target layer’s presentation layer.
        unimplemented!()
    }
}

fn id_as_option(value: id) -> Option<id> {
    if value == nil {
        None
    } else {
        Some(value)
    }
}

/// A keyframe value as its components: 1 for numbers, 2 for points and 4
/// for rectangles, so they can be interpolated and measured the same way.
type Components = Vec<f32>;

/// A transform's 2D part as translation, rotation, scale and shear, which
/// interpolate more naturally than the matrix itself.
fn transform_components(t: CGAffineTransform) -> Components {
    let angle = t.b.atan2(t.a);
    let (sin, cos) = angle.sin_cos();
    // The y axis, with the rotation undone: (shear, scale y).
    let shear = t.c * cos + t.d * sin;
    let scale_y = -t.c * sin + t.d * cos;
    vec![t.tx, t.ty, angle, t.a.hypot(t.b), shear, scale_y]
}

fn transform_from_components(value: &[f32]) -> CGAffineTransform {
    let &[tx, ty, angle, scale_x, shear, scale_y] = value else {
        unreachable!();
    };
    let (sin, cos) = angle.sin_cos();
    CGAffineTransform {
        a: cos * scale_x,
        b: sin * scale_x,
        c: shear * cos - scale_y * sin,
        d: shear * sin + scale_y * cos,
        tx,
        ty,
    }
}

fn transform_value_components(env: &mut Environment, value: id) -> Components {
    let transform: CATransform3D = msg![env; value CATransform3DValue];
    if !transform.is_affine() {
        log_once!("Warning: only the 2D part of 3D layer transforms is animated.");
    }
    transform_components(transform.affine_part())
}

/// `next`, with its angle changed by whole turns so that it's within half a
/// turn of `prev`'s, so rotations take the shortest way round.
fn unwrap_angle(prev: &Components, mut next: Components) -> Components {
    use std::f32::consts::{PI, TAU};
    let delta = next[2] - prev[2];
    next[2] -= ((delta + PI) / TAU).floor() * TAU;
    next
}

fn keyframe_components(env: &mut Environment, key_path: &str, value: id) -> Components {
    match key_path {
        "opacity"
        | "cornerRadius"
        | "transform.rotation"
        | "transform.rotation.z"
        | "transform.scale"
        | "transform.scale.x"
        | "transform.scale.y" => {
            let value: f32 = msg![env; value floatValue];
            vec![value]
        }
        "position" | "anchorPoint" => {
            let CGPoint { x, y } = msg![env; value CGPointValue];
            vec![x, y]
        }
        "bounds" => {
            let CGRect {
                origin: CGPoint { x, y },
                size: CGSize { width, height },
            } = msg![env; value CGRectValue];
            vec![x, y, width, height]
        }
        "transform" => transform_value_components(env, value),
        _ => panic!("Attempted keyframe animation on key {}", key_path),
    }
}

fn distance(a: &Components, b: &Components) -> f32 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f32>()
        .sqrt()
}

/// Set a keyframe animation's value at `time` (0 to 1) in the presentation
/// layer, following `calculationMode`, `keyTimes` and `timingFunctions`.
fn apply_keyframes(
    env: &mut Environment,
    presentation: &mut CALayerHostObject,
    key_path: &str,
    keyframes: Keyframes,
    time: f32,
) {
    let values: Vec<Components> = if keyframes.path != nil {
        assert!(
            matches!(key_path, "position" | "anchorPoint"),
            "Keyframe path for key {}",
            key_path
        );
        path_points(env, keyframes.path)
            .into_iter()
            .map(|CGPoint { x, y }| vec![x, y])
            .collect()
    } else if key_path == "contents" {
        // Images can't be interpolated, so use their indices.
        (0..keyframes.values.len())
            .map(|i| vec![i as f32])
            .collect()
    } else {
        keyframes
            .values
            .iter()
            .map(|&value| keyframe_components(env, key_path, value))
            .collect()
    };
    let values = if key_path == "transform" {
        let mut unwrapped: Vec<Components> = Vec::with_capacity(values.len());
        for value in values {
            let value = match unwrapped.last() {
                Some(prev) => unwrap_angle(prev, value),
                None => value,
            };
            unwrapped.push(value);
        }
        unwrapped
    } else {
        values
    };
    let n = values.len();
    if n == 0 {
        return;
    }

    let mut mode = keyframes.calculation_mode;
    if mode == kCAAnimationCubic || mode == kCAAnimationCubicPaced {
        log_once!("TODO: cubic keyframe animations are interpolated linearly");
        if mode == kCAAnimationCubicPaced {
            mode = kCAAnimationPaced;
        }
    }
    if key_path == "contents" && mode != kCAAnimationDiscrete {
        log_once!("TODO: contents keyframes cross-fade, but are shown discretely");
        mode = kCAAnimationDiscrete;
    }
    let paced = mode == kCAAnimationPaced;
    let discrete = mode == kCAAnimationDiscrete;

    // When each keyframe is reached, from 0 to 1.
    let even = |count: usize| -> Vec<f32> {
        (0..n)
            .map(|i| {
                if count == 0 {
                    0.0
                } else {
                    i as f32 / count as f32
                }
            })
            .collect()
    };
    let key_times = if paced {
        // Evenly paced: each segment takes time in proportion to its length.
        let mut total = 0.0;
        let mut times = vec![0.0];
        for pair in values.windows(2) {
            total += distance(&pair[0], &pair[1]);
            times.push(total);
        }
        if total > 0.0 {
            times.iter().map(|t| t / total).collect()
        } else {
            even(n - 1)
        }
    } else {
        // Discrete animations have one more key time than values.
        let expected = if discrete { n + 1 } else { n };
        match keyframes.key_times {
            Some(times) if times.len() == expected => times,
            Some(times) => {
                log!(
                    "Warning: keyframe animation has {} key times for {} values, spacing them evenly",
                    times.len(),
                    n
                );
                even(if discrete { n } else { n - 1 })
            }
            None => even(if discrete { n } else { n - 1 }),
        }
    };

    let index = key_times[..n]
        .iter()
        .rposition(|&key_time| key_time <= time)
        .unwrap_or(0);
    let value = if discrete || index + 1 >= n {
        values[index].clone()
    } else {
        let span = key_times[index + 1] - key_times[index];
        let mut amount = if span > 0.0 {
            ((time - key_times[index]) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if !paced {
            if let Some(&function) = keyframes.timing_functions.get(index) {
                amount = msg![env; function _solveForInput: amount];
            }
        }
        values[index]
            .iter()
            .zip(&values[index + 1])
            .map(|(a, b)| a + (b - a) * amount)
            .collect()
    };

    match key_path {
        "opacity" => presentation.opacity = value[0],
        "cornerRadius" => presentation.corner_radius = value[0],
        "position" => {
            presentation.position = CGPoint {
                x: value[0],
                y: value[1],
            }
        }
        "anchorPoint" => {
            presentation.anchor_point = CGPoint {
                x: value[0],
                y: value[1],
            }
        }
        "bounds" => {
            presentation.bounds = CGRect {
                origin: CGPoint {
                    x: value[0],
                    y: value[1],
                },
                size: CGSize {
                    width: value[2],
                    height: value[3],
                },
            }
        }
        "transform.rotation" | "transform.rotation.z" => {
            let current = presentation.affine_transform;
            let current_angle = current.b.atan2(current.a);
            presentation.affine_transform = current.rotate(value[0] - current_angle);
        }
        "transform.scale" | "transform.scale.x" | "transform.scale.y" => {
            set_scale(presentation, key_path, value[0])
        }
        "transform" => presentation.affine_transform = transform_from_components(&value),
        "contents" => presentation.contents = keyframes.values[value[0] as usize],
        _ => unreachable!(),
    }
}

/// A scale animation value as x and y scales: an `NSNumber` scales both,
/// and some apps use an `NSValue` of a `CGPoint` for each axis.
fn scale_value(env: &mut Environment, value: id) -> CGPoint {
    let number_class = env.objc.get_known_class("NSNumber", &mut env.mem);
    if msg![env; value isKindOfClass:number_class] {
        let scale: f32 = msg![env; value floatValue];
        CGPoint { x: scale, y: scale }
    } else {
        msg![env; value CGPointValue]
    }
}

/// The scale a `transform.scale` key path refers to (x for both axes).
fn get_scale(presentation: &CALayerHostObject, key_path: &str) -> f32 {
    let t = &presentation.affine_transform;
    if key_path == "transform.scale.y" {
        t.c.hypot(t.d)
    } else {
        t.a.hypot(t.b)
    }
}

/// Replace the scale a `transform.scale` key path refers to, keeping the
/// rotation and translation.
fn set_scale(presentation: &mut CALayerHostObject, key_path: &str, scale: f32) {
    let t = &mut presentation.affine_transform;
    let angle = t.b.atan2(t.a);
    let (sin, cos) = angle.sin_cos();
    let mut scale_x = t.a.hypot(t.b);
    let mut scale_y = t.c.hypot(t.d);
    if key_path != "transform.scale.y" {
        scale_x = scale;
    }
    if key_path != "transform.scale.x" {
        scale_y = scale;
    }
    t.a = cos * scale_x;
    t.b = sin * scale_x;
    t.c = -sin * scale_y;
    t.d = cos * scale_y;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_components_round_trip() {
        let t = CGAffineTransform::make_rotation(2.0)
            .scale(1.5, 0.5)
            .translate(3.0, 4.0);
        let back = transform_from_components(&transform_components(t));
        for (a, b) in [
            (t.a, back.a),
            (t.b, back.b),
            (t.c, back.c),
            (t.d, back.d),
            (t.tx, back.tx),
            (t.ty, back.ty),
        ] {
            assert!((a - b).abs() < 1e-5, "{t:?} != {back:?}");
        }
    }

    #[test]
    fn unwrap_angle_takes_shortest_way() {
        let prev = vec![0.0, 0.0, 3.0, 1.0, 0.0, 1.0];
        let next = vec![0.0, 0.0, -3.0, 1.0, 0.0, 1.0];
        let next = unwrap_angle(&prev, next);
        assert!((next[2] - (std::f32::consts::TAU - 3.0)).abs() < 1e-5);
    }
}
