//! The OBS filter source: registration, lifecycle callbacks and settings.

use crate::ffi::*;
use crate::obs::{self, guard, Data, GraphicsGuard, WeakSource};
use crate::props::keys::*;
use crate::render::Renderer;
use areadox_blur_core::*;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const FILTER_ID: &std::ffi::CStr = c"areadox_blur_filter";

/// Sources this filter reads from (background + mask source).
#[derive(Default)]
pub struct Sources {
    pub background: Option<WeakSource>,
    pub mask: Option<WeakSource>,
}

/// State shared between the UI thread (update), the video tick and rendering.
pub struct State {
    pub settings: Settings,
    pub sources: Sources,
    /// Whether the filter is currently shown (show/hide callbacks).
    showing: bool,
    /// Sources whose "showing" counter we incremented, so hidden sources
    /// (e.g. a background that is not in the current scene) keep updating.
    shown: Vec<WeakSource>,
}

pub struct Filter {
    pub context: *mut obs_source_t,
    pub state: Mutex<State>,
    pub renderer: Mutex<Renderer>,
    /// Re-entrancy guard: rendering the background may render this filter again.
    rendering: AtomicBool,
    /// The output of this frame is already computed (projectors, studio mode).
    rendered: AtomicBool,
}

unsafe impl Send for Filter {}
unsafe impl Sync for Filter {}

impl Filter {
    unsafe fn from<'a>(data: *mut c_void) -> &'a Filter {
        &*(data as *const Filter)
    }
}

// ---------------------------------------------------------------------------
// Settings parsing
// ---------------------------------------------------------------------------

fn parse_settings(d: &Data) -> Settings {
    let pct = |k| d.f32(k) / 100.0;
    Settings {
        algorithm: Algorithm::from_i64(d.int(ALGORITHM)),
        blur_type: BlurType::from_i64(d.int(BLUR_TYPE)),
        radius: d.f32(RADIUS),
        box_passes: d.int(BOX_PASSES).clamp(1, 5) as u32,
        angle: d.f32(ANGLE),
        pixel_shape: PixelShape::from_i64(d.int(PIXEL_SHAPE)),
        zoom_center: (pct(ZOOM_X), pct(ZOOM_Y)),
        zoom_inactive: pct(ZOOM_INACTIVE),
        tilt_center: pct(TILT_CENTER),
        tilt_width: pct(TILT_WIDTH),
        tilt_feather: pct(TILT_FEATHER),
        tilt_angle: d.f32(TILT_ANGLE),
        background_uuid: d.string(BACKGROUND),
        background_mode: BackgroundMode::from_i64(d.int(BACKGROUND_MODE)),
        glass_edge: pct(GLASS_EDGE),
        mask: MaskType::from_i64(d.int(MASK_TYPE)),
        mask_invert: d.bool(MASK_INVERT),
        crop: [
            d.f32(CROP_LEFT),
            d.f32(CROP_RIGHT),
            d.f32(CROP_TOP),
            d.f32(CROP_BOTTOM),
        ],
        rect_center: (pct(RECT_X), pct(RECT_Y)),
        rect_size: (pct(RECT_W), pct(RECT_H)),
        rect_angle: d.f32(RECT_ANGLE),
        corner_radius: d.f32(CORNER_RADIUS),
        circle_center: (pct(CIRCLE_X), pct(CIRCLE_Y)),
        circle_radius: pct(CIRCLE_RADIUS),
        feather: d.f32(FEATHER),
        mask_source_uuid: d.string(MASK_SOURCE),
        mask_image_path: d.string(MASK_IMAGE),
        mask_channel: MaskChannel::from_i64(d.int(MASK_CHANNEL)),
        mask_multiplier: d.f32(MASK_MULTIPLIER),
    }
    .sanitized()
}

pub fn set_defaults(d: &Data) {
    let s = Settings::default();
    d.default_int(ALGORITHM, s.algorithm as i64);
    d.default_int(BLUR_TYPE, s.blur_type as i64);
    d.default_f64(RADIUS, s.radius as f64);
    d.default_int(BOX_PASSES, s.box_passes as i64);
    d.default_f64(ANGLE, 0.0);
    d.default_int(PIXEL_SHAPE, 0);
    d.default_f64(ZOOM_X, 50.0);
    d.default_f64(ZOOM_Y, 50.0);
    d.default_f64(ZOOM_INACTIVE, 0.0);
    d.default_f64(TILT_CENTER, 50.0);
    d.default_f64(TILT_WIDTH, 25.0);
    d.default_f64(TILT_FEATHER, 20.0);
    d.default_f64(TILT_ANGLE, 0.0);
    d.default_string(BACKGROUND, c"");
    d.default_int(BACKGROUND_MODE, 0);
    d.default_f64(GLASS_EDGE, 5.0);
    d.default_int(MASK_TYPE, 0);
    d.default_bool(MASK_INVERT, false);
    for k in [CROP_LEFT, CROP_RIGHT, CROP_TOP, CROP_BOTTOM] {
        d.default_f64(k, 0.0);
    }
    d.default_f64(RECT_X, 50.0);
    d.default_f64(RECT_Y, 50.0);
    d.default_f64(RECT_W, 50.0);
    d.default_f64(RECT_H, 50.0);
    d.default_f64(RECT_ANGLE, 0.0);
    d.default_f64(CORNER_RADIUS, 0.0);
    d.default_f64(CIRCLE_X, 50.0);
    d.default_f64(CIRCLE_Y, 50.0);
    d.default_f64(CIRCLE_RADIUS, 25.0);
    d.default_f64(FEATHER, 2.0);
    d.default_string(MASK_SOURCE, c"");
    d.default_string(MASK_IMAGE, c"");
    d.default_int(MASK_CHANNEL, 0);
    d.default_f64(MASK_MULTIPLIER, 1.0);
}

// ---------------------------------------------------------------------------
// Source references / "showing" bookkeeping
// ---------------------------------------------------------------------------

impl State {
    /// Resolve sources by UUID if they are not resolved yet (sources may be
    /// created after the filter when a scene collection loads).
    fn resolve_sources(&mut self) -> bool {
        let mut changed = false;
        let wants_bg = !self.settings.background_uuid.is_empty();
        if wants_bg
            && self
                .sources
                .background
                .as_ref()
                .map_or(true, |w| w.get().is_none())
        {
            self.sources.background = WeakSource::find(&self.settings.background_uuid);
            changed |= self.sources.background.is_some();
        } else if !wants_bg && self.sources.background.is_some() {
            self.sources.background = None;
            changed = true;
        }
        let wants_mask =
            self.settings.mask == MaskType::Source && !self.settings.mask_source_uuid.is_empty();
        if wants_mask
            && self
                .sources
                .mask
                .as_ref()
                .map_or(true, |w| w.get().is_none())
        {
            self.sources.mask = WeakSource::find(&self.settings.mask_source_uuid);
            changed |= self.sources.mask.is_some();
        } else if !wants_mask && self.sources.mask.is_some() {
            self.sources.mask = None;
            changed = true;
        }
        changed
    }

    /// Make sure exactly the referenced sources have their showing counter
    /// incremented while this filter is shown.
    fn sync_showing(&mut self, self_parent: *mut obs_source_t) {
        let mut wanted: Vec<*mut obs_weak_source_t> = Vec::new();
        if self.showing {
            for w in [&self.sources.background, &self.sources.mask]
                .into_iter()
                .flatten()
            {
                // Never touch our own parent or any scene/group: a scene may
                // contain this filter's source, and keeping it "showing" from
                // here would keep ourselves showing forever (a cycle).
                if let Some(s) = w.get() {
                    let is_scene = unsafe { obs_source_get_type(s.0) } == OBS_SOURCE_TYPE_SCENE;
                    if s.0 != self_parent && !is_scene && !wanted.contains(&w.raw()) {
                        wanted.push(w.raw());
                    }
                }
            }
        }
        // Remove entries that are no longer wanted.
        self.shown.retain(|w| {
            if wanted.contains(&w.raw()) {
                return true;
            }
            if let Some(s) = w.get() {
                unsafe { obs_source_dec_showing(s.0) };
            }
            false
        });
        // Add new ones.
        for raw in wanted {
            if self.shown.iter().any(|w| w.raw() == raw) {
                continue;
            }
            if let Some(w) = WeakSource::from_raw_addref(raw) {
                if let Some(s) = w.get() {
                    unsafe { obs_source_inc_showing(s.0) };
                    self.shown.push(w);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

unsafe extern "C" fn get_name(_type_data: *mut c_void) -> *const c_char {
    crate::text(c"Filter.Name")
}

unsafe extern "C" fn create(settings: *mut obs_data_t, source: *mut obs_source_t) -> *mut c_void {
    guard(ptr::null_mut(), || {
        let renderer = {
            let _g = GraphicsGuard::enter();
            Renderer::new(crate::current_module())
        };
        let filter = Box::new(Filter {
            context: source,
            state: Mutex::new(State {
                settings: Settings::default(),
                sources: Sources::default(),
                showing: false,
                shown: Vec::new(),
            }),
            renderer: Mutex::new(renderer),
            rendering: AtomicBool::new(false),
            rendered: AtomicBool::new(false),
        });
        let raw = Box::into_raw(filter) as *mut c_void;
        update(raw, settings);
        raw
    })
}

unsafe extern "C" fn destroy(data: *mut c_void) {
    guard((), || {
        let filter = Box::from_raw(data as *mut Filter);
        {
            let mut st = filter.state.lock().unwrap_or_else(|e| e.into_inner());
            st.showing = false;
            st.sync_showing(ptr::null_mut());
            st.sources = Sources::default();
        }
        let _g = GraphicsGuard::enter();
        drop(filter); // drops all GPU resources inside the graphics context
    })
}

unsafe extern "C" fn update(data: *mut c_void, settings: *mut obs_data_t) {
    guard((), || {
        let filter = Filter::from(data);
        let new = parse_settings(&Data(settings));
        let parent = obs_filter_get_parent(filter.context);
        let mut st = filter.state.lock().unwrap_or_else(|e| e.into_inner());
        if new.background_uuid != st.settings.background_uuid {
            st.sources.background = None;
        }
        if new.mask_source_uuid != st.settings.mask_source_uuid || new.mask != st.settings.mask {
            st.sources.mask = None;
        }
        st.settings = new;
        st.resolve_sources();
        st.sync_showing(parent);
        filter.rendered.store(false, Ordering::Release);
    })
}

unsafe extern "C" fn get_defaults(settings: *mut obs_data_t) {
    guard((), || set_defaults(&Data(settings)))
}

unsafe extern "C" fn get_properties(data: *mut c_void) -> *mut obs_properties_t {
    guard(ptr::null_mut(), || {
        let parent = if data.is_null() {
            ptr::null_mut()
        } else {
            obs_filter_get_parent(Filter::from(data).context)
        };
        crate::props::build(parent)
    })
}

unsafe extern "C" fn show(data: *mut c_void) {
    guard((), || {
        let filter = Filter::from(data);
        let parent = obs_filter_get_parent(filter.context);
        let mut st = filter.state.lock().unwrap_or_else(|e| e.into_inner());
        st.showing = true;
        st.sync_showing(parent);
    })
}

unsafe extern "C" fn hide(data: *mut c_void) {
    guard((), || {
        let filter = Filter::from(data);
        let mut st = filter.state.lock().unwrap_or_else(|e| e.into_inner());
        st.showing = false;
        st.sync_showing(ptr::null_mut());
    })
}

unsafe extern "C" fn video_tick(data: *mut c_void, _seconds: f32) {
    guard((), || {
        let filter = Filter::from(data);
        filter.rendered.store(false, Ordering::Release);
        // Cheap: only does work while a configured source is missing.
        if let Ok(mut st) = filter.state.try_lock() {
            let missing = (!st.settings.background_uuid.is_empty()
                && st.sources.background.is_none())
                || (st.settings.mask == MaskType::Source
                    && !st.settings.mask_source_uuid.is_empty()
                    && st.sources.mask.is_none());
            if missing && st.resolve_sources() {
                let parent = obs_filter_get_parent(filter.context);
                st.sync_showing(parent);
            }
        }
    })
}

unsafe extern "C" fn video_render(data: *mut c_void, _effect: *mut gs_effect_t) {
    let filter = Filter::from(data);
    // Re-entrant call (our own parent is part of the background): draw unfiltered.
    if filter.rendering.swap(true, Ordering::AcqRel) {
        obs_source_skip_video_filter(filter.context);
        return;
    }
    guard((), || {
        let Ok(mut renderer) = filter.renderer.try_lock() else {
            obs_source_skip_video_filter(filter.context);
            return;
        };
        if filter.rendered.load(Ordering::Acquire) && renderer.draw_cached() {
            return;
        }
        let (settings, bg, mask) = {
            let st = filter.state.lock().unwrap_or_else(|e| e.into_inner());
            (
                st.settings.clone(),
                st.sources.background.as_ref().and_then(|w| w.get()),
                st.sources.mask.as_ref().and_then(|w| w.get()),
            )
        };
        if renderer.render(filter.context, &settings, bg.as_ref(), mask.as_ref()) {
            filter.rendered.store(true, Ordering::Release);
        }
    });
    filter.rendering.store(false, Ordering::Release);
}

unsafe extern "C" fn video_get_color_space(
    data: *mut c_void,
    count: usize,
    preferred: *const gs_color_space,
) -> gs_color_space {
    let filter = Filter::from(data);
    let target = obs_filter_get_target(filter.context);
    if target.is_null() {
        return if count > 0 { *preferred } else { GS_CS_SRGB };
    }
    obs_source_get_color_space(target, count, preferred)
}

unsafe extern "C" fn filter_remove(data: *mut c_void, _parent: *mut obs_source_t) {
    hide(data);
}

pub fn register() {
    // SAFETY: obs_source_info is plain old data; all-zero is "no callback".
    let mut info: obs_source_info = unsafe { std::mem::zeroed() };
    info.id = FILTER_ID.as_ptr();
    info.type_ = OBS_SOURCE_TYPE_FILTER;
    info.output_flags = OBS_SOURCE_VIDEO | OBS_SOURCE_SRGB;
    info.get_name = Some(get_name);
    info.create = Some(create);
    info.destroy = Some(destroy);
    info.update = Some(update);
    info.get_defaults = Some(get_defaults);
    info.get_properties = Some(get_properties);
    info.show = Some(show);
    info.hide = Some(hide);
    info.video_tick = Some(video_tick);
    info.video_render = Some(video_render);
    info.video_get_color_space = Some(video_get_color_space);
    info.filter_remove = Some(filter_remove);
    unsafe { obs_register_source_s(&info, std::mem::size_of::<obs_source_info>()) };
    obs::info!("registered filter {}", FILTER_ID.to_string_lossy());
}
