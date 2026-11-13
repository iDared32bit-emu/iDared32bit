/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Utilities for presenting frames to the window using an abstract OpenGL ES
//! implementation.

use super::gles11_raw as gles11; // constants and types only
use super::GLES;
use crate::matrix::Matrix;
use crate::window::DeviceOrientation;
use std::time::{Duration, Instant};

pub struct FpsCounter {
    time: std::time::Instant,
    frames: u32,
}
impl FpsCounter {
    pub fn start() -> Self {
        FpsCounter {
            time: Instant::now(),
            frames: 0,
        }
    }

    pub fn count_frame(&mut self, label: std::fmt::Arguments<'_>) {
        self.frames += 1;
        let now = Instant::now();
        let duration = now - self.time;
        if duration >= Duration::from_secs(1) {
            self.time = now;
            echo!(
                "{} FPS: {:.2}",
                label,
                std::mem::take(&mut self.frames) as f32 / duration.as_secs_f32()
            );
        }
    }
}

/// Texture coordinates for the two-triangle quad used to present a frame.
pub struct TextureCoordinates {
    tex_coords: [f32; 12],
}

impl TextureCoordinates {
    pub const fn unnormalized() -> &'static TextureCoordinates {
        &TextureCoordinates {
            tex_coords: [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }
    }

    pub const fn normalized(
        render_width: u32,
        render_height: u32,
        texture_width: u32,
        texture_height: u32,
        device_orientation: DeviceOrientation,
    ) -> TextureCoordinates {
        let u_max = (render_width as f32) / texture_width as f32;
        let v_max = (render_height as f32) / texture_height as f32;

        #[rustfmt::skip]
        let tex_coords: [f32; 12] = match device_orientation {
            DeviceOrientation::Portrait => [
                0.0,   0.0,
                0.0,   v_max,
                u_max, 0.0,
                u_max, 0.0,
                0.0,   v_max,
                u_max, v_max,
            ],
            DeviceOrientation::PortraitUpsideDown => [
                u_max, v_max,
                u_max, 0.0,
                0.0,   v_max,
                0.0,   v_max,
                u_max, 0.0,
                0.0,   0.0,
            ],
            DeviceOrientation::LandscapeLeft => [
                0.0,   v_max,
                u_max, v_max,
                0.0,   0.0,
                0.0,   0.0,
                u_max, v_max,
                u_max, 0.0,
            ],
            DeviceOrientation::LandscapeRight => [
                u_max, 0.0,
                0.0,   0.0,
                u_max, v_max,
                u_max, v_max,
                0.0,   0.0,
                0.0,   v_max,
            ],
        };

        Self { tex_coords }
    }
}

/// The on-screen Home button to draw over the frame (see
/// [crate::window::Window::home_button_overlay]): its centre and radius in
/// drawable pixels from the top left, and the drawable's size.
pub struct HomeButtonOverlay {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub pressed: bool,
    pub screen_width: u32,
    pub screen_height: u32,
}

/// Present the the latest frame (e.g. the app's splash screen or rendering
/// output), provided as a texture bound to `GL_TEXTURE_2D`, by drawing it on
/// the window. It may be rotated, scaled and/or letterboxed as necessary. The
/// virtual cursor and the on-screen Home button are also drawn if they should
/// be currently visible.
///
/// The provided context must be current.
///
/// If `normalized_texture_coords` is provided, then texture coordinates will
/// be normalized with the provided UV width and height factors.
pub unsafe fn present_frame(
    gles: &mut dyn GLES,
    viewport: (u32, u32, u32, u32),
    rotation_matrix: Matrix<2>,
    virtual_cursor_visible_at: Option<(f32, f32, bool)>,
    home_button: Option<HomeButtonOverlay>,
    normalized_texture_coords: Option<&TextureCoordinates>,
) {
    // While this is a generic utility, it is closely tied to
    // crate::frameworks::opengles::eagl::present_renderbuffer, which handles
    // backing up and restoring OpenGL ES state that this function might touch,
    // so these need to be updated in tandem.

    use gles11::types::*;

    // Draw the quad
    gles.Viewport(
        viewport.0 as _,
        viewport.1 as _,
        viewport.2 as _,
        viewport.3 as _,
    );

    gles.ClearColor(0.0, 0.0, 0.0, 1.0);
    gles.Clear(gles11::COLOR_BUFFER_BIT | gles11::DEPTH_BUFFER_BIT | gles11::STENCIL_BUFFER_BIT);
    gles.BindBuffer(gles11::ARRAY_BUFFER, 0);
    let vertices: [f32; 12] = [
        -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0,
    ];
    gles.EnableClientState(gles11::VERTEX_ARRAY);
    gles.VertexPointer(2, gles11::FLOAT, 0, vertices.as_ptr() as *const GLvoid);

    let (tex_coords, matrix) = match normalized_texture_coords {
        Some(coords) => (coords.tex_coords.as_ptr(), Matrix::<4>::identity()),
        None => (
            TextureCoordinates::unnormalized().tex_coords.as_ptr(),
            Matrix::<4>::from(&rotation_matrix),
        ),
    };

    gles.EnableClientState(gles11::TEXTURE_COORD_ARRAY);
    gles.TexCoordPointer(2, gles11::FLOAT, 0, tex_coords as *const GLvoid);
    gles.MatrixMode(gles11::TEXTURE);
    gles.LoadMatrixf(matrix.columns().as_ptr() as *const _);
    gles.Enable(gles11::TEXTURE_2D);
    gles.DrawArrays(gles11::TRIANGLES, 0, 6);
    // clean this up so we don't need to worry about it in e.g. Core Animation
    gles.LoadIdentity();

    // Display virtual cursor
    if let Some((x, y, pressed)) = virtual_cursor_visible_at {
        let (_, _, vw, vh) = viewport;

        gles.DisableClientState(gles11::TEXTURE_COORD_ARRAY);
        gles.Disable(gles11::TEXTURE_2D);

        gles.Enable(gles11::BLEND);
        gles.BlendFunc(gles11::ONE, gles11::ONE_MINUS_SRC_ALPHA);
        gles.Color4f(0.0, 0.0, 0.0, if pressed { 2.0 / 3.0 } else { 1.0 / 3.0 });

        let radius = 10.0;

        let mut vertices = vertices;
        for i in (0..vertices.len()).step_by(2) {
            vertices[i] = (vertices[i] * radius + x) / (vw as f32 / 2.0) - 1.0;
            vertices[i + 1] = 1.0 - (vertices[i + 1] * radius + y) / (vh as f32 / 2.0);
        }
        gles.VertexPointer(2, gles11::FLOAT, 0, vertices.as_ptr() as *const GLvoid);
        gles.DrawArrays(gles11::TRIANGLES, 0, 6);
    }

    // Display the on-screen Home button, in the black bars outside the app:
    // a ring with an X inside.
    if let Some(button) = home_button {
        let HomeButtonOverlay {
            x,
            y,
            radius,
            pressed,
            screen_width: width,
            screen_height: height,
        } = button;
        gles.Viewport(0, 0, width as _, height as _);

        gles.DisableClientState(gles11::TEXTURE_COORD_ARRAY);
        gles.Disable(gles11::TEXTURE_2D);
        gles.Enable(gles11::BLEND);
        gles.BlendFunc(gles11::ONE, gles11::ONE_MINUS_SRC_ALPHA);
        // Light grey, brighter while pressed.
        let alpha = if pressed { 0.9 } else { 0.4 };

        // Pixels from the top left to normalized device co-ordinates.
        let to_ndc = |px: f32, py: f32| {
            [
                px / width as f32 * 2.0 - 1.0,
                1.0 - py / height as f32 * 2.0,
            ]
        };

        // Drawn smoothly at its size in pixels, lined up with them, then
        // shown as a texture. A power of two, as GLES 1.1 may need.
        let size = ((radius * 2.0).ceil() as u32 + 2).next_power_of_two();
        let (left, top) = (
            (x - size as f32 / 2.0).floor(),
            (y - size as f32 / 2.0).floor(),
        );
        let pixmap = home_button_pixmap(size, x - left, y - top, radius, alpha);

        let mut old_texture = 0;
        gles.GetIntegerv(gles11::TEXTURE_BINDING_2D, &mut old_texture);
        let mut texture = 0;
        gles.GenTextures(1, &mut texture);
        gles.BindTexture(gles11::TEXTURE_2D, texture);
        for (pname, param) in [
            (gles11::TEXTURE_MIN_FILTER, gles11::NEAREST),
            (gles11::TEXTURE_MAG_FILTER, gles11::NEAREST),
            (gles11::TEXTURE_WRAP_S, gles11::CLAMP_TO_EDGE),
            (gles11::TEXTURE_WRAP_T, gles11::CLAMP_TO_EDGE),
        ] {
            gles.TexParameteri(gles11::TEXTURE_2D, pname, param as _);
        }
        gles.TexImage2D(
            gles11::TEXTURE_2D,
            0,
            gles11::RGBA as _,
            size as _,
            size as _,
            0,
            gles11::RGBA,
            gles11::UNSIGNED_BYTE,
            pixmap.data().as_ptr() as *const GLvoid,
        );

        let (right, bottom) = (left + size as f32, top + size as f32);
        let mut quad = Vec::with_capacity(12);
        for (px, py) in [
            (left, top),
            (right, top),
            (left, bottom),
            (right, top),
            (right, bottom),
            (left, bottom),
        ] {
            quad.extend(to_ndc(px, py));
        }
        let tex_coords: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        gles.Enable(gles11::TEXTURE_2D);
        gles.EnableClientState(gles11::TEXTURE_COORD_ARRAY);
        gles.TexCoordPointer(2, gles11::FLOAT, 0, tex_coords.as_ptr() as *const GLvoid);
        gles.Color4f(1.0, 1.0, 1.0, 1.0);
        gles.VertexPointer(2, gles11::FLOAT, 0, quad.as_ptr() as *const GLvoid);
        gles.DrawArrays(gles11::TRIANGLES, 0, 6);
        gles.DeleteTextures(1, &texture);
        gles.BindTexture(gles11::TEXTURE_2D, old_texture as _);
    }
}

/// The Home button: a ring with an X inside (a plus sign turned 45°, which
/// looks the same whichever way the device is held), with smooth edges.
/// Premultiplied, centred on `cx`, `cy`.
fn home_button_pixmap(size: u32, cx: f32, cy: f32, radius: f32, alpha: f32) -> tiny_skia::Pixmap {
    use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};
    let mut pixmap = Pixmap::new(size, size).unwrap();
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba(0.85, 0.85, 0.85, alpha).unwrap());
    paint.anti_alias = true;
    let ring = PathBuilder::from_circle(cx, cy, radius * 0.93).unwrap();
    let stroke = Stroke {
        width: radius * 0.14,
        ..Default::default()
    };
    pixmap.stroke_path(&ring, &paint, &stroke, Transform::identity(), None);
    // One path, so where the arms cross isn't drawn twice.
    let arm = radius * 0.44 * std::f32::consts::FRAC_1_SQRT_2;
    let mut cross = PathBuilder::new();
    cross.move_to(cx - arm, cy - arm);
    cross.line_to(cx + arm, cy + arm);
    cross.move_to(cx + arm, cy - arm);
    cross.line_to(cx - arm, cy + arm);
    let stroke = Stroke {
        width: radius * 0.26,
        ..Default::default()
    };
    let cross = cross.finish().unwrap();
    pixmap.stroke_path(&cross, &paint, &stroke, Transform::identity(), None);
    pixmap
}
