//! Hand-written bindings for the parts of libobs (OBS Studio 33) this plugin uses.
//!
//! Only the types and functions that are actually needed are declared. The
//! layout of `obs_source_info` is checked at compile time against offsets
//! measured from the OBS 33.0.3 C headers (identical on all 64-bit targets).
//!
#![allow(non_camel_case_types, non_upper_case_globals, dead_code)]

use std::ffi::{c_char, c_int, c_longlong, c_void};

// ---------------------------------------------------------------------------
// Opaque handles
// ---------------------------------------------------------------------------
macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(#[repr(C)] pub struct $name { _p: [u8; 0] })*
    };
}
opaque!(
    obs_module_t,
    obs_source_t,
    obs_weak_source_t,
    obs_data_t,
    obs_properties_t,
    obs_property_t,
    lookup_t,
    gs_effect_t,
    gs_eparam_t,
    gs_texture_t,
    gs_texrender_t,
);

// ---------------------------------------------------------------------------
// Constants / enums (C enums are passed as 32-bit ints on all targets)
// ---------------------------------------------------------------------------
pub type obs_source_type = u32;
pub const OBS_SOURCE_TYPE_FILTER: obs_source_type = 1;
pub const OBS_SOURCE_TYPE_SCENE: obs_source_type = 3;

pub const OBS_SOURCE_VIDEO: u32 = 1 << 0;
pub const OBS_SOURCE_SRGB: u32 = 1 << 15;

pub type obs_allow_direct_render = u32;
pub const OBS_NO_DIRECT_RENDERING: obs_allow_direct_render = 0;

pub type obs_combo_type = u32;
pub const OBS_COMBO_TYPE_LIST: obs_combo_type = 2;
pub type obs_combo_format = u32;
pub const OBS_COMBO_FORMAT_INT: obs_combo_format = 1;
pub const OBS_COMBO_FORMAT_STRING: obs_combo_format = 3;
pub type obs_group_type = u32;
pub const OBS_GROUP_NORMAL: obs_group_type = 1;
pub type obs_path_type = u32;
pub const OBS_PATH_FILE: obs_path_type = 0;

pub type gs_color_format = u32;
pub const GS_RGBA: gs_color_format = 3;
pub const GS_RGBA16F: gs_color_format = 9;

pub type gs_color_space = u32;
pub const GS_CS_SRGB: gs_color_space = 0;
pub const GS_CS_SRGB_16F: gs_color_space = 1;
pub const GS_CS_709_EXTENDED: gs_color_space = 2;
pub const GS_CS_709_SCRGB: gs_color_space = 3;

pub type gs_zstencil_format = u32;
pub const GS_ZS_NONE: gs_zstencil_format = 0;

pub type gs_blend_type = u32;
pub const GS_BLEND_ZERO: gs_blend_type = 0;
pub const GS_BLEND_ONE: gs_blend_type = 1;
pub const GS_BLEND_SRCALPHA: gs_blend_type = 4;
pub const GS_BLEND_INVSRCALPHA: gs_blend_type = 5;

pub const GS_CLEAR_COLOR: u32 = 1 << 0;

pub type obs_base_effect = u32;
pub const OBS_EFFECT_DEFAULT: obs_base_effect = 0;

pub const LOG_ERROR: c_int = 100;
pub const LOG_WARNING: c_int = 200;
pub const LOG_INFO: c_int = 300;
pub const LOG_DEBUG: c_int = 400;

/// `MAKE_SEMANTIC_VERSION(33, 0, 0)` – the libobs API this plugin targets.
pub const LIBOBS_API_MAJOR_VER: u32 = 33;
pub const LIBOBS_API_MINOR_VER: u32 = 0;
pub const LIBOBS_API_VER: u32 = (LIBOBS_API_MAJOR_VER << 24) | (LIBOBS_API_MINOR_VER << 16);

/// Equivalent of the static inline `gs_get_format_from_space` in graphics.h.
pub fn gs_get_format_from_space(space: gs_color_space) -> gs_color_format {
    match space {
        GS_CS_SRGB_16F | GS_CS_709_EXTENDED | GS_CS_709_SCRGB => GS_RGBA16F,
        _ => GS_RGBA,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct vec2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
pub struct vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

// ---------------------------------------------------------------------------
// obs_source_info (OBS 33 layout)
// ---------------------------------------------------------------------------
type Unused = Option<unsafe extern "C" fn()>;

pub type obs_source_enum_proc_t = Option<
    unsafe extern "C" fn(parent: *mut obs_source_t, child: *mut obs_source_t, param: *mut c_void),
>;

#[repr(C)]
pub struct obs_source_info {
    pub id: *const c_char,
    pub type_: obs_source_type,
    pub output_flags: u32,
    pub get_name: Option<unsafe extern "C" fn(type_data: *mut c_void) -> *const c_char>,
    pub create: Option<
        unsafe extern "C" fn(settings: *mut obs_data_t, source: *mut obs_source_t) -> *mut c_void,
    >,
    pub destroy: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub get_width: Option<unsafe extern "C" fn(data: *mut c_void) -> u32>,
    pub get_height: Option<unsafe extern "C" fn(data: *mut c_void) -> u32>,
    pub get_defaults: Option<unsafe extern "C" fn(settings: *mut obs_data_t)>,
    pub get_properties: Option<unsafe extern "C" fn(data: *mut c_void) -> *mut obs_properties_t>,
    pub update: Option<unsafe extern "C" fn(data: *mut c_void, settings: *mut obs_data_t)>,
    pub activate: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub deactivate: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub show: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub hide: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub video_tick: Option<unsafe extern "C" fn(data: *mut c_void, seconds: f32)>,
    pub video_render: Option<unsafe extern "C" fn(data: *mut c_void, effect: *mut gs_effect_t)>,
    pub filter_video: Unused,
    pub filter_audio: Unused,
    pub enum_active_sources: Unused,
    pub save: Unused,
    pub load: Unused,
    pub mouse_click: Unused,
    pub mouse_move: Unused,
    pub mouse_wheel: Unused,
    pub focus: Unused,
    pub key_click: Unused,
    pub filter_remove: Option<unsafe extern "C" fn(data: *mut c_void, source: *mut obs_source_t)>,
    pub type_data: *mut c_void,
    pub free_type_data: Unused,
    pub audio_render: Unused,
    pub enum_all_sources: Unused,
    pub transition_start: Unused,
    pub transition_stop: Unused,
    pub get_defaults2: Unused,
    pub get_properties2: Unused,
    pub audio_mix: Unused,
    pub icon_type: u32,
    pub media_play_pause: Unused,
    pub media_restart: Unused,
    pub media_stop: Unused,
    pub media_next: Unused,
    pub media_previous: Unused,
    pub media_get_duration: Unused,
    pub media_get_time: Unused,
    pub media_set_time: Unused,
    pub media_get_state: Unused,
    pub version: u32,
    pub unversioned_id: *const c_char,
    pub missing_files: Unused,
    pub video_get_color_space: Option<
        unsafe extern "C" fn(
            data: *mut c_void,
            count: usize,
            preferred: *const gs_color_space,
        ) -> gs_color_space,
    >,
    pub filter_add: Unused,
    pub get_dark_icon: Unused,
    pub get_light_icon: Unused,
}

// Offsets measured with offsetof() against the OBS 33.0.3 headers.
const _: () = {
    use std::mem::{offset_of, size_of};
    assert!(size_of::<obs_source_info>() == 424);
    assert!(offset_of!(obs_source_info, output_flags) == 12);
    assert!(offset_of!(obs_source_info, video_render) == 120);
    assert!(offset_of!(obs_source_info, filter_remove) == 208);
    assert!(offset_of!(obs_source_info, type_data) == 216);
    assert!(offset_of!(obs_source_info, icon_type) == 288);
    assert!(offset_of!(obs_source_info, version) == 368);
    assert!(offset_of!(obs_source_info, video_get_color_space) == 392);
    assert!(offset_of!(obs_source_info, get_light_icon) == 416);
};

pub type obs_property_modified_t = Option<
    unsafe extern "C" fn(
        props: *mut obs_properties_t,
        property: *mut obs_property_t,
        settings: *mut obs_data_t,
    ) -> bool,
>;
pub type obs_enum_proc_t =
    Option<unsafe extern "C" fn(param: *mut c_void, source: *mut obs_source_t) -> bool>;

// ---------------------------------------------------------------------------
// Functions exported by libobs (obs.dll / libobs.so / libobs.framework)
//
// Windows: `raw-dylib` lets rustc generate the import entries for obs.dll
// itself – no obs.lib / OBS SDK is required to build.
// Linux/macOS: symbols are resolved when OBS loads the plugin.
// ---------------------------------------------------------------------------

// `blog` is variadic. We only ever call it as blog(level, "%s", msg); on
// Windows x64/ARM64 that call is ABI-identical to a non-variadic call, which
// keeps it compatible with raw-dylib linking.
#[cfg(windows)]
#[link(name = "obs", kind = "raw-dylib")]
extern "C" {
    pub fn blog(log_level: c_int, format: *const c_char, msg: *const c_char);
}
#[cfg(not(windows))]
extern "C" {
    pub fn blog(log_level: c_int, format: *const c_char, ...);
}

#[cfg_attr(windows, link(name = "obs", kind = "raw-dylib"))]
extern "C" {
    // --- core / module ---
    pub fn obs_register_source_s(info: *const obs_source_info, size: usize);
    pub fn obs_find_module_file(module: *mut obs_module_t, file: *const c_char) -> *mut c_char;
    pub fn obs_module_load_locale(
        module: *mut obs_module_t,
        default_locale: *const c_char,
        locale: *const c_char,
    ) -> *mut lookup_t;
    pub fn text_lookup_getstr(
        lookup: *mut lookup_t,
        lookup_val: *const c_char,
        out: *mut *const c_char,
    ) -> bool;
    pub fn text_lookup_destroy(lookup: *mut lookup_t);
    pub fn bfree(ptr: *mut c_void);
    pub fn obs_enter_graphics();
    pub fn obs_leave_graphics();
    pub fn obs_get_base_effect(effect: obs_base_effect) -> *mut gs_effect_t;
    pub fn obs_enum_sources(enum_proc: obs_enum_proc_t, param: *mut c_void);
    pub fn obs_enum_scenes(enum_proc: obs_enum_proc_t, param: *mut c_void);

    // --- data ---
    pub fn obs_data_get_int(data: *mut obs_data_t, name: *const c_char) -> c_longlong;
    pub fn obs_data_get_double(data: *mut obs_data_t, name: *const c_char) -> f64;
    pub fn obs_data_get_bool(data: *mut obs_data_t, name: *const c_char) -> bool;
    pub fn obs_data_get_string(data: *mut obs_data_t, name: *const c_char) -> *const c_char;
    pub fn obs_data_set_default_int(data: *mut obs_data_t, name: *const c_char, val: c_longlong);
    pub fn obs_data_set_default_double(data: *mut obs_data_t, name: *const c_char, val: f64);
    pub fn obs_data_set_default_bool(data: *mut obs_data_t, name: *const c_char, val: bool);
    pub fn obs_data_set_default_string(
        data: *mut obs_data_t,
        name: *const c_char,
        val: *const c_char,
    );

    // --- properties ---
    pub fn obs_properties_create() -> *mut obs_properties_t;
    pub fn obs_properties_get(
        props: *mut obs_properties_t,
        property: *const c_char,
    ) -> *mut obs_property_t;
    pub fn obs_properties_add_list(
        props: *mut obs_properties_t,
        name: *const c_char,
        description: *const c_char,
        type_: obs_combo_type,
        format: obs_combo_format,
    ) -> *mut obs_property_t;
    pub fn obs_property_list_add_int(
        p: *mut obs_property_t,
        name: *const c_char,
        val: c_longlong,
    ) -> usize;
    pub fn obs_property_list_add_string(
        p: *mut obs_property_t,
        name: *const c_char,
        val: *const c_char,
    ) -> usize;
    pub fn obs_properties_add_float_slider(
        props: *mut obs_properties_t,
        name: *const c_char,
        description: *const c_char,
        min: f64,
        max: f64,
        step: f64,
    ) -> *mut obs_property_t;
    pub fn obs_properties_add_int_slider(
        props: *mut obs_properties_t,
        name: *const c_char,
        description: *const c_char,
        min: c_int,
        max: c_int,
        step: c_int,
    ) -> *mut obs_property_t;
    pub fn obs_properties_add_bool(
        props: *mut obs_properties_t,
        name: *const c_char,
        description: *const c_char,
    ) -> *mut obs_property_t;
    pub fn obs_properties_add_group(
        props: *mut obs_properties_t,
        name: *const c_char,
        description: *const c_char,
        type_: obs_group_type,
        group: *mut obs_properties_t,
    ) -> *mut obs_property_t;
    pub fn obs_properties_add_path(
        props: *mut obs_properties_t,
        name: *const c_char,
        description: *const c_char,
        type_: obs_path_type,
        filter: *const c_char,
        default_path: *const c_char,
    ) -> *mut obs_property_t;
    pub fn obs_property_set_modified_callback(
        p: *mut obs_property_t,
        modified: obs_property_modified_t,
    );
    pub fn obs_property_set_visible(p: *mut obs_property_t, visible: bool);
    pub fn obs_property_set_description(p: *mut obs_property_t, description: *const c_char);
    pub fn obs_property_list_clear(p: *mut obs_property_t);
    pub fn obs_property_float_set_suffix(p: *mut obs_property_t, suffix: *const c_char);
    pub fn obs_property_int_set_suffix(p: *mut obs_property_t, suffix: *const c_char);
    pub fn obs_property_set_long_description(
        p: *mut obs_property_t,
        long_description: *const c_char,
    );

    // --- sources ---
    pub fn obs_source_get_name(source: *const obs_source_t) -> *const c_char;
    pub fn obs_source_get_uuid(source: *const obs_source_t) -> *const c_char;
    pub fn obs_get_source_by_uuid(uuid: *const c_char) -> *mut obs_source_t;
    pub fn obs_get_source_by_name(name: *const c_char) -> *mut obs_source_t;
    pub fn obs_source_get_weak_source(source: *mut obs_source_t) -> *mut obs_weak_source_t;
    pub fn obs_weak_source_get_source(weak: *mut obs_weak_source_t) -> *mut obs_source_t;
    pub fn obs_weak_source_release(weak: *mut obs_weak_source_t);
    pub fn obs_weak_source_addref(weak: *mut obs_weak_source_t);
    pub fn obs_source_release(source: *mut obs_source_t);
    pub fn obs_source_get_output_flags(source: *const obs_source_t) -> u32;
    pub fn obs_source_get_type(source: *const obs_source_t) -> obs_source_type;
    pub fn obs_filter_get_target(filter: *const obs_source_t) -> *mut obs_source_t;
    pub fn obs_filter_get_parent(filter: *const obs_source_t) -> *mut obs_source_t;
    pub fn obs_source_get_base_width(source: *mut obs_source_t) -> u32;
    pub fn obs_source_get_base_height(source: *mut obs_source_t) -> u32;
    pub fn obs_source_get_width(source: *mut obs_source_t) -> u32;
    pub fn obs_source_get_height(source: *mut obs_source_t) -> u32;
    pub fn obs_source_video_render(source: *mut obs_source_t);
    pub fn obs_source_get_color_space(
        source: *mut obs_source_t,
        count: usize,
        preferred_spaces: *const gs_color_space,
    ) -> gs_color_space;
    pub fn obs_source_process_filter_begin_with_color_space(
        filter: *mut obs_source_t,
        format: gs_color_format,
        space: gs_color_space,
        allow_direct: obs_allow_direct_render,
    ) -> bool;
    pub fn obs_source_process_filter_tech_end(
        filter: *mut obs_source_t,
        effect: *mut gs_effect_t,
        width: u32,
        height: u32,
        tech_name: *const c_char,
    );
    pub fn obs_source_skip_video_filter(filter: *mut obs_source_t);
    pub fn obs_source_inc_showing(source: *mut obs_source_t);
    pub fn obs_source_dec_showing(source: *mut obs_source_t);

    // --- graphics ---
    pub fn gs_effect_create_from_file(
        file: *const c_char,
        error_string: *mut *mut c_char,
    ) -> *mut gs_effect_t;
    pub fn gs_effect_destroy(effect: *mut gs_effect_t);
    pub fn gs_effect_get_param_by_name(
        effect: *const gs_effect_t,
        name: *const c_char,
    ) -> *mut gs_eparam_t;
    pub fn gs_effect_set_texture(param: *mut gs_eparam_t, val: *mut gs_texture_t);
    pub fn gs_effect_set_texture_srgb(param: *mut gs_eparam_t, val: *mut gs_texture_t);
    pub fn gs_effect_set_float(param: *mut gs_eparam_t, val: f32);
    pub fn gs_effect_set_int(param: *mut gs_eparam_t, val: c_int);
    pub fn gs_effect_set_vec2(param: *mut gs_eparam_t, val: *const vec2);
    pub fn gs_effect_set_vec4(param: *mut gs_eparam_t, val: *const vec4);
    pub fn gs_effect_loop(effect: *mut gs_effect_t, name: *const c_char) -> bool;
    pub fn gs_texrender_create(
        format: gs_color_format,
        zsformat: gs_zstencil_format,
    ) -> *mut gs_texrender_t;
    pub fn gs_texrender_destroy(texrender: *mut gs_texrender_t);
    pub fn gs_texrender_reset(texrender: *mut gs_texrender_t);
    pub fn gs_texrender_begin_with_color_space(
        texrender: *mut gs_texrender_t,
        cx: u32,
        cy: u32,
        space: gs_color_space,
    ) -> bool;
    pub fn gs_texrender_end(texrender: *mut gs_texrender_t);
    pub fn gs_texrender_get_texture(texrender: *const gs_texrender_t) -> *mut gs_texture_t;
    pub fn gs_texture_create_from_file(file: *const c_char) -> *mut gs_texture_t;
    pub fn gs_texture_destroy(tex: *mut gs_texture_t);
    pub fn gs_draw_sprite(tex: *mut gs_texture_t, flip: u32, width: u32, height: u32);
    pub fn gs_ortho(left: f32, right: f32, top: f32, bottom: f32, znear: f32, zfar: f32);
    pub fn gs_clear(clear_flags: u32, color: *const vec4, depth: f32, stencil: u8);
    pub fn gs_blend_state_push();
    pub fn gs_blend_state_pop();
    pub fn gs_blend_function(src: gs_blend_type, dest: gs_blend_type);
    pub fn gs_blend_function_separate(
        src_c: gs_blend_type,
        dest_c: gs_blend_type,
        src_a: gs_blend_type,
        dest_a: gs_blend_type,
    );
    pub fn gs_enable_framebuffer_srgb(enable: bool);
    pub fn gs_framebuffer_srgb_enabled() -> bool;
    pub fn gs_get_linear_srgb() -> bool;
}
