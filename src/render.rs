//! GPU pipeline of the filter.
//!
//! ```text
//! source ──capture──► input ─┬─(background? "Combined": source over bg)──► base
//!                            │                                              │
//!   background ──render──► bg ┴─(background? "Frosted glass": blur bg only)  │
//!                                                                           ▼
//!                                         blur (Gaussian / Box / Dual Kawase / Pixelate)
//!                                                                           │
//!                       frosted glass: sharp source over blurred bg ◄───────┤
//!                       tilt shift:    sharp ↔ blurred along a band ◄───────┤
//!                       mask:          input ↔ result ◄─────────────────────┘
//!                                                                           ▼
//!                                                       output (premultiplied → straight)
//! ```
//! All intermediate textures contain premultiplied alpha in linear light.

use crate::ffi::*;
use crate::obs::{draw, log_warn, BlendGuard, Effect, SrgbGuard, StrongSource, TexRender};
use areadox_blur_core::*;
use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Slot {
    Input,
    Background,
    MaskSource,
    Composite,
    Sharp,
    Tilt,
    Out,
    FullA,
    FullB,
    Ping(u8),
    Level(u32),
}

#[derive(Clone, Copy)]
enum Fx {
    Blur,
    Mix,
}

type Tex = *mut gs_texture_t;

pub struct Renderer {
    blur: Option<Effect>,
    mix: Option<Effect>,
    targets: HashMap<Slot, TexRender>,
    /// Frame counter and the frame each target was last used in (unused
    /// targets are freed after a while, e.g. when the background is removed).
    frame: u64,
    last_used: HashMap<Slot, u64>,
    mask_image: Option<(String, Tex)>,
    /// Final texture of the current frame (premultiplied) and its size.
    output: Option<(Tex, u32, u32)>,
}

unsafe impl Send for Renderer {}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some((_, t)) = self.mask_image.take() {
            if !t.is_null() {
                unsafe { gs_texture_destroy(t) };
            }
        }
    }
}

/// Everything a pass needs to know about the frame.
#[derive(Clone, Copy)]
struct Frame {
    w: u32,
    h: u32,
    fmt: gs_color_format,
    space: gs_color_space,
}

impl Renderer {
    /// Must be called inside the graphics context.
    pub fn new(module: *mut obs_module_t) -> Renderer {
        let blur = Effect::load(module, "effects/blur.effect");
        let mix = Effect::load(module, "effects/mix.effect");
        Renderer {
            blur,
            mix,
            targets: HashMap::new(),
            frame: 0,
            last_used: HashMap::new(),
            mask_image: None,
            output: None,
        }
    }

    fn target(&mut self, slot: Slot, fmt: gs_color_format) -> &mut TexRender {
        self.last_used.insert(slot, self.frame);
        let recreate = self.targets.get(&slot).map_or(true, |t| t.format() != fmt);
        if recreate {
            self.targets.insert(slot, TexRender::new(fmt));
        }
        self.targets.get_mut(&slot).expect("inserted above")
    }

    /// One full-screen shader pass into `slot` (size `w` x `h`).
    #[allow(clippy::too_many_arguments)]
    fn pass(
        &mut self,
        f: Frame,
        slot: Slot,
        w: u32,
        h: u32,
        fx: Fx,
        technique: &CStr,
        setup: impl FnOnce(&Effect),
    ) -> Tex {
        self.target(slot, f.fmt);
        let Renderer {
            blur, mix, targets, ..
        } = self;
        let effect = match fx {
            Fx::Blur => blur.as_ref(),
            Fx::Mix => mix.as_ref(),
        };
        let (Some(effect), Some(rt)) = (effect, targets.get_mut(&slot)) else {
            return ptr::null_mut();
        };
        let _blend = BlendGuard::copy();
        let _srgb = SrgbGuard::set(true);
        if !rt.begin(w, h, w as f32, h as f32, f.space) {
            return ptr::null_mut();
        }
        setup(effect);
        draw(effect, technique, ptr::null_mut(), w, h);
        rt.end();
        rt.texture()
    }

    /// Render another source, stretched to the frame size, premultiplied.
    fn render_source(&mut self, f: Frame, slot: Slot, src: &StrongSource) -> Tex {
        let (sw, sh) = unsafe { (obs_source_get_width(src.0), obs_source_get_height(src.0)) };
        if sw == 0 || sh == 0 {
            return ptr::null_mut();
        }
        let rt = self.target(slot, f.fmt);
        let _blend = BlendGuard::premultiply();
        if !rt.begin(f.w, f.h, sw as f32, sh as f32, f.space) {
            return ptr::null_mut();
        }
        unsafe { obs_source_video_render(src.0) };
        rt.end();
        rt.texture()
    }

    /// Free render targets that have not been used for a while.
    fn collect_garbage(&mut self) {
        const KEEP_FRAMES: u64 = 120;
        let frame = self.frame;
        let last_used = &mut self.last_used;
        self.targets.retain(|slot, _| {
            let keep = last_used
                .get(slot)
                .is_some_and(|&f| frame - f <= KEEP_FRAMES);
            if !keep {
                last_used.remove(slot);
            }
            keep
        });
    }

    /// Re-draw the already computed output of this frame.
    pub fn draw_cached(&mut self) -> bool {
        self.draw_output()
    }

    fn draw_output(&self) -> bool {
        let (Some((tex, w, h)), Some(mix)) = (self.output, self.mix.as_ref()) else {
            return false;
        };
        if tex.is_null() {
            return false;
        }
        unsafe {
            let linear = gs_get_linear_srgb();
            let _srgb = SrgbGuard::set(linear);
            let p = gs_effect_get_param_by_name(mix.0, c"image".as_ptr());
            if linear {
                gs_effect_set_texture_srgb(p, tex);
            } else {
                gs_effect_set_texture(p, tex);
            }
            draw(mix, c"Output", tex, w, h);
        }
        true
    }

    /// Does the filter change anything with these settings?
    fn is_noop(s: &Settings, has_bg: bool) -> bool {
        let no_blur = match s.algorithm {
            Algorithm::Pixelate => s.radius < 1.5,
            _ => s.radius < 0.25,
        };
        no_blur && !has_bg
    }

    /// Render the filter for this frame. Returns true if output was drawn.
    pub fn render(
        &mut self,
        ctx: *mut obs_source_t,
        s: &Settings,
        bg: Option<&StrongSource>,
        mask_src: Option<&StrongSource>,
    ) -> bool {
        self.output = None;
        self.frame += 1;
        if self.frame % 60 == 0 {
            self.collect_garbage();
        }
        unsafe {
            let target = obs_filter_get_target(ctx);
            let parent = obs_filter_get_parent(ctx);
            if target.is_null() || parent.is_null() {
                return false;
            }
            let (w, h) = (
                obs_source_get_base_width(target),
                obs_source_get_base_height(target),
            );
            if w == 0
                || h == 0
                || self.blur.is_none()
                || self.mix.is_none()
                || Self::is_noop(s, bg.is_some())
            {
                obs_source_skip_video_filter(ctx);
                return false;
            }
            let preferred = [GS_CS_SRGB, GS_CS_SRGB_16F, GS_CS_709_EXTENDED];
            let space = obs_source_get_color_space(target, preferred.len(), preferred.as_ptr());
            let f = Frame {
                w,
                h,
                fmt: gs_get_format_from_space(space),
                space,
            };

            // 1. capture the filter input (premultiplied)
            if !obs_source_process_filter_begin_with_color_space(
                ctx,
                f.fmt,
                space,
                OBS_NO_DIRECT_RENDERING,
            ) {
                return false;
            }
            let input = {
                let rt = self.target(Slot::Input, f.fmt);
                let _blend = BlendGuard::copy();
                if !rt.begin(w, h, w as f32, h as f32, space) {
                    obs_source_skip_video_filter(ctx);
                    return false;
                }
                obs_source_process_filter_tech_end(
                    ctx,
                    obs_get_base_effect(OBS_EFFECT_DEFAULT),
                    w,
                    h,
                    c"Draw".as_ptr(),
                );
                rt.end();
                rt.texture()
            };

            // 2. background
            let bg_tex = bg.map_or(ptr::null_mut(), |b| {
                self.render_source(f, Slot::Background, b)
            });
            let has_bg = !bg_tex.is_null();
            let glass = has_bg && s.background_mode == BackgroundMode::FrostedGlass;
            let base = if has_bg && !glass {
                self.pass(f, Slot::Composite, w, h, Fx::Mix, c"Composite", |e| {
                    e.tex(c"image", input);
                    e.tex(c"blurred", bg_tex);
                })
            } else {
                input
            };

            // 3. blur
            let mut result = self.blur(f, s, if glass { bg_tex } else { base });
            if result.is_null() {
                // a pass failed (e.g. out of GPU memory): show the source unchanged
                result = input;
            }
            let ty = s.algorithm.effective_type(s.blur_type);
            let mut sharp = base;
            if glass {
                if ty == BlurType::TiltShift {
                    sharp = self.pass(f, Slot::Sharp, w, h, Fx::Mix, c"Glass", |e| {
                        e.tex(c"image", input);
                        e.tex(c"blurred", bg_tex);
                        e.f(c"glass_edge", s.glass_edge);
                    });
                }
                result = self.pass(f, Slot::Composite, w, h, Fx::Mix, c"Glass", |e| {
                    e.tex(c"image", input);
                    e.tex(c"blurred", result);
                    e.f(c"glass_edge", s.glass_edge);
                });
            }

            // 4. tilt shift
            if ty == BlurType::TiltShift {
                let (wf, hf) = (w as f32, h as f32);
                let a = s.tilt_angle.to_radians();
                result = self.pass(f, Slot::Tilt, w, h, Fx::Mix, c"Tilt", |e| {
                    e.tex(c"image", sharp);
                    e.tex(c"blurred", result);
                    e.v2(c"uv_size", wf, hf);
                    e.v2(c"tilt_center", wf * 0.5, s.tilt_center * hf);
                    e.v2(c"tilt_normal", -a.sin(), a.cos());
                    e.f(c"tilt_half", s.tilt_width * hf * 0.5);
                    e.f(c"tilt_feather", s.tilt_feather * hf);
                    e.f(c"invert", 0.0);
                });
            }

            // 5. mask
            if s.mask != MaskType::None {
                if let Some(masked) = self.mask(f, s, input, result, mask_src) {
                    result = masked;
                }
            }

            if result.is_null() {
                result = input;
            }
            self.output = Some((result, w, h));
            self.draw_output()
        }
    }

    fn blur(&mut self, f: Frame, s: &Settings, src: Tex) -> Tex {
        let kernel = if s.algorithm == Algorithm::Box { 1 } else { 0 };
        let iterations = if s.algorithm == Algorithm::Box {
            s.box_passes
        } else {
            1
        };
        match s.algorithm {
            Algorithm::Gaussian | Algorithm::Box => match s.algorithm.effective_type(s.blur_type) {
                BlurType::Zoom => self.zoom(f, s, src, kernel),
                BlurType::Directional => self.directional(f, s, src, kernel, iterations),
                BlurType::Area | BlurType::TiltShift => self.separable(
                    f,
                    src,
                    shader_kernel_radius(s.algorithm, s.radius),
                    kernel,
                    iterations,
                ),
            },
            Algorithm::DualKawase => self.kawase(f, src, s.radius),
            Algorithm::Pixelate => {
                if s.radius < 1.5 {
                    return src;
                }
                let tech = if s.pixel_shape == PixelShape::Hexagon {
                    c"PixelHex"
                } else {
                    c"PixelSquare"
                };
                let (w, h) = (f.w, f.h);
                self.pass(f, Slot::FullA, w, h, Fx::Blur, tech, |e| {
                    e.tex(c"image", src);
                    e.v2(c"uv_size", w as f32, h as f32);
                    e.f(c"pixel_size", s.radius);
                })
            }
        }
    }

    fn separable(&mut self, f: Frame, src: Tex, radius: f32, kernel: i32, iterations: u32) -> Tex {
        if radius < 0.25 {
            return src;
        }
        let plan = plan_separable(radius, iterations, f.w.min(f.h));
        let mut cur = src;
        // downsample
        for l in 1..=plan.levels {
            let (lw, lh) = level_size(f.w, f.h, l);
            cur = self.copy(f, Slot::Level(l), lw, lh, cur);
        }
        let (cw, ch) = level_size(f.w, f.h, plan.levels);
        for _ in 0..plan.iterations {
            for (slot, step) in [
                (Slot::Ping(0), (1.0 / cw as f32, 0.0)),
                (Slot::Ping(1), (0.0, 1.0 / ch as f32)),
            ] {
                let input = cur;
                cur = self.pass(f, slot, cw, ch, Fx::Blur, c"Blur1D", |e| {
                    e.tex(c"image", input);
                    e.v2(c"texel_step", step.0, step.1);
                    e.f(c"radius", plan.pass_radius);
                    e.i(c"kernel_type", kernel);
                });
            }
        }
        // upsample back level by level (smoother than one big bilinear jump)
        if plan.levels > 0 {
            for l in (1..plan.levels).rev() {
                let (lw, lh) = level_size(f.w, f.h, l);
                cur = self.copy(f, Slot::Level(l), lw, lh, cur);
            }
            cur = self.copy(f, Slot::FullA, f.w, f.h, cur);
        }
        cur
    }

    fn copy(&mut self, f: Frame, slot: Slot, w: u32, h: u32, src: Tex) -> Tex {
        self.pass(f, slot, w, h, Fx::Blur, c"Copy", |e| e.tex(c"image", src))
    }

    fn directional(
        &mut self,
        f: Frame,
        s: &Settings,
        src: Tex,
        kernel: i32,
        iterations: u32,
    ) -> Tex {
        if s.radius < 0.25 {
            return src;
        }
        let (dx, dy) = direction(s.angle);
        let r = shader_kernel_radius(s.algorithm, s.radius) / (iterations.max(1) as f32).sqrt();
        let mut cur = src;
        for i in 0..iterations.max(1) {
            let slot = if i % 2 == 0 { Slot::FullA } else { Slot::FullB };
            let input = cur;
            cur = self.pass(f, slot, f.w, f.h, Fx::Blur, c"Blur1D", |e| {
                e.tex(c"image", input);
                e.v2(c"texel_step", dx / f.w as f32, dy / f.h as f32);
                e.f(c"radius", r);
                e.i(c"kernel_type", kernel);
            });
        }
        cur
    }

    fn zoom(&mut self, f: Frame, s: &Settings, src: Tex, kernel: i32) -> Tex {
        if s.radius < 0.5 {
            return src;
        }
        self.pass(f, Slot::FullA, f.w, f.h, Fx::Blur, c"Zoom", |e| {
            e.tex(c"image", src);
            e.v2(c"uv_size", f.w as f32, f.h as f32);
            e.v2(c"center", s.zoom_center.0, s.zoom_center.1);
            e.f(c"inactive", s.zoom_inactive);
            e.f(c"radius", shader_kernel_radius(s.algorithm, s.radius));
            e.i(c"kernel_type", kernel);
        })
    }

    fn kawase(&mut self, f: Frame, src: Tex, radius: f32) -> Tex {
        let Some(plan) = plan_kawase(radius, f.w.min(f.h)) else {
            return src;
        };
        let mut cur = src;
        for i in 1..=plan.iterations {
            let (sw, sh) = level_size(f.w, f.h, i - 1);
            let (tw, th) = level_size(f.w, f.h, i);
            let input = cur;
            cur = self.pass(f, Slot::Level(i), tw, th, Fx::Blur, c"KawaseDown", |e| {
                e.tex(c"image", input);
                e.v2(
                    c"half_texel",
                    plan.offset / sw as f32,
                    plan.offset / sh as f32,
                );
            });
        }
        for i in (0..plan.iterations).rev() {
            let (sw, sh) = level_size(f.w, f.h, i + 1);
            let (tw, th) = level_size(f.w, f.h, i);
            let slot = if i == 0 { Slot::FullA } else { Slot::Level(i) };
            let input = cur;
            cur = self.pass(f, slot, tw, th, Fx::Blur, c"KawaseUp", |e| {
                e.tex(c"image", input);
                e.v2(
                    c"half_texel",
                    plan.offset / sw as f32,
                    plan.offset / sh as f32,
                );
            });
        }
        cur
    }

    fn mask(
        &mut self,
        f: Frame,
        s: &Settings,
        input: Tex,
        blurred: Tex,
        mask_src: Option<&StrongSource>,
    ) -> Option<Tex> {
        let (wf, hf) = (f.w as f32, f.h as f32);
        let common = |e: &Effect| {
            e.tex(c"image", input);
            e.tex(c"blurred", blurred);
            e.v2(c"uv_size", wf, hf);
            e.f(c"feather", s.feather);
            e.f(c"invert", if s.mask_invert { 1.0 } else { 0.0 });
        };
        let rect = match s.mask {
            MaskType::Crop => Some(crop_rect(s.crop, s.corner_radius, wf, hf)),
            MaskType::Rect => Some(rect_from_settings(s, wf, hf)),
            _ => None,
        };
        if let Some(r) = rect {
            return Some(self.pass(f, Slot::Out, f.w, f.h, Fx::Mix, c"Rect", |e| {
                common(e);
                e.v2(c"rect_center", r.center.0, r.center.1);
                e.v2(c"rect_half", r.half_size.0, r.half_size.1);
                e.f(c"rect_angle", r.angle_rad);
                e.f(c"corner", r.corner);
            }));
        }
        match s.mask {
            MaskType::Circle => Some(self.pass(f, Slot::Out, f.w, f.h, Fx::Mix, c"Circle", |e| {
                common(e);
                e.v2(
                    c"circle_center",
                    s.circle_center.0 * wf,
                    s.circle_center.1 * hf,
                );
                e.f(c"circle_radius", s.circle_radius * wf.min(hf));
            })),
            MaskType::Source | MaskType::Image => {
                let tex = if s.mask == MaskType::Source {
                    self.render_source(f, Slot::MaskSource, mask_src?)
                } else {
                    self.mask_image(&s.mask_image_path)
                };
                if tex.is_null() {
                    return None;
                }
                let weights = channel_weights(s.mask_channel);
                Some(self.pass(f, Slot::Out, f.w, f.h, Fx::Mix, c"Texture", |e| {
                    common(e);
                    // mask values are meant perceptually: sample without sRGB decoding
                    e.tex_raw(c"mask_image", tex);
                    e.v4(c"channel_weights", weights);
                    e.f(
                        c"use_luminance",
                        if s.mask_channel == MaskChannel::Luminance {
                            1.0
                        } else {
                            0.0
                        },
                    );
                    e.f(c"multiplier", s.mask_multiplier);
                }))
            }
            _ => None,
        }
    }

    /// Load (and cache) the image used as mask.
    fn mask_image(&mut self, path: &str) -> Tex {
        if let Some((p, t)) = &self.mask_image {
            if p == path {
                return *t;
            }
        }
        if let Some((_, old)) = self.mask_image.take() {
            if !old.is_null() {
                unsafe { gs_texture_destroy(old) };
            }
        }
        let tex = if path.is_empty() {
            ptr::null_mut()
        } else {
            let c = crate::obs::cstring(path);
            let t = unsafe { gs_texture_create_from_file(c.as_ptr()) };
            if t.is_null() {
                log_warn!("could not load mask image '{path}'");
            }
            t
        };
        self.mask_image = Some((path.to_string(), tex));
        tex
    }
}
