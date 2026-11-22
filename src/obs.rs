//! Small safe(ish) helpers on top of the raw libobs FFI.

use crate::ffi::*;
use std::ffi::{c_void, CStr, CString};
use std::ptr;

/// Write a message to the OBS log, prefixed with the plugin name.
pub fn log(level: std::ffi::c_int, msg: &str) {
    let msg = CString::new(format!("[areadox-blur] {msg}")).unwrap_or_default();
    unsafe { blog(level, c"%s".as_ptr(), msg.as_ptr()) };
}

macro_rules! info { ($($t:tt)*) => { $crate::obs::log($crate::ffi::LOG_INFO, &format!($($t)*)) } }
macro_rules! log_warn { ($($t:tt)*) => { $crate::obs::log($crate::ffi::LOG_WARNING, &format!($($t)*)) } }
pub(crate) use {info, log_warn};

/// Run an OBS callback body, turning a Rust panic into a log entry instead of
/// unwinding across the C boundary (which would abort OBS).
pub fn guard<R>(fallback: R, f: impl FnOnce() -> R) -> R {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(e) => {
            let what = e
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".into());
            log(LOG_ERROR, &format!("panic in callback: {what}"));
            fallback
        }
    }
}

/// Convert a possibly-null C string into an owned Rust string.
pub unsafe fn to_string(p: *const std::ffi::c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

pub fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// Typed accessors for an `obs_data_t`.
pub struct Data(pub *mut obs_data_t);

impl Data {
    pub fn int(&self, k: &CStr) -> i64 {
        unsafe { obs_data_get_int(self.0, k.as_ptr()) }
    }
    pub fn f32(&self, k: &CStr) -> f32 {
        unsafe { obs_data_get_double(self.0, k.as_ptr()) as f32 }
    }
    pub fn bool(&self, k: &CStr) -> bool {
        unsafe { obs_data_get_bool(self.0, k.as_ptr()) }
    }
    pub fn string(&self, k: &CStr) -> String {
        unsafe { to_string(obs_data_get_string(self.0, k.as_ptr())) }
    }
    pub fn default_int(&self, k: &CStr, v: i64) {
        unsafe { obs_data_set_default_int(self.0, k.as_ptr(), v) }
    }
    pub fn default_f64(&self, k: &CStr, v: f64) {
        unsafe { obs_data_set_default_double(self.0, k.as_ptr(), v) }
    }
    pub fn default_bool(&self, k: &CStr, v: bool) {
        unsafe { obs_data_set_default_bool(self.0, k.as_ptr(), v) }
    }
    pub fn default_string(&self, k: &CStr, v: &CStr) {
        unsafe { obs_data_set_default_string(self.0, k.as_ptr(), v.as_ptr()) }
    }
}

/// An owned weak reference to a source. Released on drop.
pub struct WeakSource(*mut obs_weak_source_t);
unsafe impl Send for WeakSource {}

impl WeakSource {
    /// Look a source up by UUID (falls back to the name, for settings that
    /// were written by hand or by older versions).
    pub fn find(uuid_or_name: &str) -> Option<WeakSource> {
        if uuid_or_name.is_empty() {
            return None;
        }
        let c = cstring(uuid_or_name);
        unsafe {
            let mut src = obs_get_source_by_uuid(c.as_ptr());
            if src.is_null() {
                src = obs_get_source_by_name(c.as_ptr());
            }
            if src.is_null() {
                return None;
            }
            let weak = obs_source_get_weak_source(src);
            obs_source_release(src);
            (!weak.is_null()).then_some(WeakSource(weak))
        }
    }

    pub fn raw(&self) -> *mut obs_weak_source_t {
        self.0
    }

    /// Take an additional reference on an existing weak pointer.
    pub fn from_raw_addref(raw: *mut obs_weak_source_t) -> Option<WeakSource> {
        if raw.is_null() {
            return None;
        }
        unsafe { obs_weak_source_addref(raw) };
        Some(WeakSource(raw))
    }

    /// Upgrade to a strong reference for the duration of the returned guard.
    pub fn get(&self) -> Option<StrongSource> {
        let s = unsafe { obs_weak_source_get_source(self.0) };
        (!s.is_null()).then_some(StrongSource(s))
    }
}

impl Drop for WeakSource {
    fn drop(&mut self) {
        unsafe { obs_weak_source_release(self.0) }
    }
}

/// A strong (ref-counted) source reference, released on drop.
pub struct StrongSource(pub *mut obs_source_t);
impl Drop for StrongSource {
    fn drop(&mut self) {
        unsafe { obs_source_release(self.0) }
    }
}

/// Resolve a file in the plugin's data directory.
pub fn module_file(module: *mut obs_module_t, rel: &str) -> Option<CString> {
    let rel = cstring(rel);
    unsafe {
        let p = obs_find_module_file(module, rel.as_ptr());
        if p.is_null() {
            return None;
        }
        let s = CStr::from_ptr(p).to_owned();
        bfree(p as *mut c_void);
        Some(s)
    }
}

// ---------------------------------------------------------------------------
// Graphics helpers (must only be used inside the graphics context)
// ---------------------------------------------------------------------------

/// An effect file loaded from the plugin data directory.
pub struct Effect(pub *mut gs_effect_t);
unsafe impl Send for Effect {}

impl Effect {
    pub fn load(module: *mut obs_module_t, rel: &str) -> Option<Effect> {
        let Some(path) = module_file(module, rel) else {
            log_warn!("effect file {rel} not found in plugin data directory");
            return None;
        };
        let mut err: *mut std::ffi::c_char = ptr::null_mut();
        let e = unsafe { gs_effect_create_from_file(path.as_ptr(), &mut err) };
        if !err.is_null() {
            let msg = unsafe { to_string(err) };
            unsafe { bfree(err as *mut c_void) };
            if e.is_null() {
                log_warn!("failed to compile {rel}: {msg}");
            }
        }
        (!e.is_null()).then_some(Effect(e))
    }

    fn param(&self, name: &CStr) -> *mut gs_eparam_t {
        unsafe { gs_effect_get_param_by_name(self.0, name.as_ptr()) }
    }
    pub fn f(&self, name: &CStr, v: f32) {
        let p = self.param(name);
        if !p.is_null() {
            unsafe { gs_effect_set_float(p, v) }
        }
    }
    pub fn i(&self, name: &CStr, v: i32) {
        let p = self.param(name);
        if !p.is_null() {
            unsafe { gs_effect_set_int(p, v) }
        }
    }
    pub fn v2(&self, name: &CStr, x: f32, y: f32) {
        let p = self.param(name);
        if !p.is_null() {
            unsafe { gs_effect_set_vec2(p, &vec2 { x, y }) }
        }
    }
    pub fn v4(&self, name: &CStr, v: [f32; 4]) {
        let p = self.param(name);
        if !p.is_null() {
            unsafe {
                gs_effect_set_vec4(
                    p,
                    &vec4 {
                        x: v[0],
                        y: v[1],
                        z: v[2],
                        w: v[3],
                    },
                )
            }
        }
    }
    /// Bind a texture; sampled in linear light (sRGB view) like OBS does for
    /// `OBS_SOURCE_SRGB` sources.
    pub fn tex(&self, name: &CStr, t: *mut gs_texture_t) {
        let p = self.param(name);
        if !p.is_null() {
            unsafe { gs_effect_set_texture_srgb(p, t) }
        }
    }
}

impl Effect {
    /// Bind a texture without sRGB decoding (raw stored values).
    pub fn tex_raw(&self, name: &CStr, t: *mut gs_texture_t) {
        let p = self.param(name);
        if !p.is_null() {
            unsafe { gs_effect_set_texture(p, t) }
        }
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        unsafe { gs_effect_destroy(self.0) }
    }
}

/// Render target wrapper.
pub struct TexRender {
    ptr: *mut gs_texrender_t,
    format: gs_color_format,
}
unsafe impl Send for TexRender {}

impl TexRender {
    pub fn new(format: gs_color_format) -> TexRender {
        TexRender {
            ptr: unsafe { gs_texrender_create(format, GS_ZS_NONE) },
            format,
        }
    }
    pub fn format(&self) -> gs_color_format {
        self.format
    }
    pub fn texture(&self) -> *mut gs_texture_t {
        unsafe { gs_texrender_get_texture(self.ptr) }
    }

    /// Reset and begin rendering into this target. Sets an orthographic
    /// projection of `ortho_w` x `ortho_h`, clears it to transparent and
    /// selects plain "copy" blending. Returns false if the target could not be
    /// (re)allocated.
    pub fn begin(
        &mut self,
        w: u32,
        h: u32,
        ortho_w: f32,
        ortho_h: f32,
        space: gs_color_space,
    ) -> bool {
        unsafe {
            gs_texrender_reset(self.ptr);
            if !gs_texrender_begin_with_color_space(self.ptr, w, h, space) {
                return false;
            }
            let zero = vec4::default();
            gs_clear(GS_CLEAR_COLOR, &zero, 0.0, 0);
            gs_ortho(0.0, ortho_w, 0.0, ortho_h, -100.0, 100.0);
        }
        true
    }
    pub fn end(&mut self) {
        unsafe { gs_texrender_end(self.ptr) }
    }
}

impl Drop for TexRender {
    fn drop(&mut self) {
        unsafe { gs_texrender_destroy(self.ptr) }
    }
}

/// RAII guard: push blend state + set it, restore on drop.
pub struct BlendGuard;
impl BlendGuard {
    /// Plain copy (no blending).
    pub fn copy() -> BlendGuard {
        unsafe {
            gs_blend_state_push();
            gs_blend_function(GS_BLEND_ONE, GS_BLEND_ZERO);
        }
        BlendGuard
    }
    /// Standard OBS blending into an empty target – yields premultiplied alpha.
    pub fn premultiply() -> BlendGuard {
        unsafe {
            gs_blend_state_push();
            gs_blend_function_separate(
                GS_BLEND_SRCALPHA,
                GS_BLEND_INVSRCALPHA,
                GS_BLEND_ONE,
                GS_BLEND_INVSRCALPHA,
            );
        }
        BlendGuard
    }
}
impl Drop for BlendGuard {
    fn drop(&mut self) {
        unsafe { gs_blend_state_pop() }
    }
}

/// RAII guard: enable/disable framebuffer sRGB conversion, restore on drop.
pub struct SrgbGuard(bool);
impl SrgbGuard {
    pub fn set(enable: bool) -> SrgbGuard {
        let prev = unsafe { gs_framebuffer_srgb_enabled() };
        unsafe { gs_enable_framebuffer_srgb(enable) };
        SrgbGuard(prev)
    }
}
impl Drop for SrgbGuard {
    fn drop(&mut self) {
        unsafe { gs_enable_framebuffer_srgb(self.0) }
    }
}

/// Enter the graphics context for the guard's lifetime.
pub struct GraphicsGuard;
impl GraphicsGuard {
    pub fn enter() -> GraphicsGuard {
        unsafe { obs_enter_graphics() };
        GraphicsGuard
    }
}
impl Drop for GraphicsGuard {
    fn drop(&mut self) {
        unsafe { obs_leave_graphics() }
    }
}

/// Draw `tex` as a full-target sprite of `w` x `h` with `technique`.
pub fn draw(effect: &Effect, technique: &CStr, tex: *mut gs_texture_t, w: u32, h: u32) {
    unsafe {
        while gs_effect_loop(effect.0, technique.as_ptr()) {
            gs_draw_sprite(tex, 0, w, h);
        }
    }
}
