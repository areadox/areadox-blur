//! The filter's properties dialog.

use crate::ffi::*;
use crate::obs::{cstring, to_string, Data};
use crate::text;
use areadox_blur_core::*;
use std::ffi::{c_void, CStr};

/// Setting keys (stored in the scene collection – do not rename).
pub mod keys {
    use std::ffi::CStr;
    pub const ALGORITHM: &CStr = c"blur_algorithm";
    pub const BLUR_TYPE: &CStr = c"blur_type";
    pub const RADIUS: &CStr = c"radius";
    pub const BOX_PASSES: &CStr = c"box_passes";
    pub const ANGLE: &CStr = c"direction";
    pub const PIXEL_SHAPE: &CStr = c"pixelate_shape";
    pub const ZOOM_X: &CStr = c"zoom_center_x";
    pub const ZOOM_Y: &CStr = c"zoom_center_y";
    pub const ZOOM_INACTIVE: &CStr = c"zoom_inactive_radius";
    pub const TILT_CENTER: &CStr = c"tilt_center";
    pub const TILT_WIDTH: &CStr = c"tilt_width";
    pub const TILT_FEATHER: &CStr = c"tilt_feather";
    pub const TILT_ANGLE: &CStr = c"tilt_angle";
    pub const BACKGROUND: &CStr = c"background";
    pub const BACKGROUND_MODE: &CStr = c"background_mode";
    pub const GLASS_EDGE: &CStr = c"glass_edge";
    pub const MASK_TYPE: &CStr = c"mask_type";
    pub const MASK_INVERT: &CStr = c"mask_invert";
    pub const CROP_LEFT: &CStr = c"mask_crop_left";
    pub const CROP_RIGHT: &CStr = c"mask_crop_right";
    pub const CROP_TOP: &CStr = c"mask_crop_top";
    pub const CROP_BOTTOM: &CStr = c"mask_crop_bottom";
    pub const RECT_X: &CStr = c"mask_rect_center_x";
    pub const RECT_Y: &CStr = c"mask_rect_center_y";
    pub const RECT_W: &CStr = c"mask_rect_width";
    pub const RECT_H: &CStr = c"mask_rect_height";
    pub const RECT_ANGLE: &CStr = c"mask_rect_angle";
    pub const CORNER_RADIUS: &CStr = c"mask_corner_radius";
    pub const CIRCLE_X: &CStr = c"mask_circle_center_x";
    pub const CIRCLE_Y: &CStr = c"mask_circle_center_y";
    pub const CIRCLE_RADIUS: &CStr = c"mask_circle_radius";
    pub const FEATHER: &CStr = c"mask_feather";
    pub const MASK_SOURCE: &CStr = c"mask_source";
    pub const MASK_IMAGE: &CStr = c"mask_image";
    pub const MASK_CHANNEL: &CStr = c"mask_channel";
    pub const MASK_MULTIPLIER: &CStr = c"mask_multiplier";

    // group names
    pub const G_ZOOM: &CStr = c"group_zoom";
    pub const G_TILT: &CStr = c"group_tilt";
    pub const G_BACKGROUND: &CStr = c"group_background";
    pub const G_MASK: &CStr = c"group_mask";
}
use keys::*;

unsafe fn slider(
    p: *mut obs_properties_t,
    k: &CStr,
    label: &CStr,
    min: f64,
    max: f64,
    step: f64,
    suffix: &CStr,
) {
    let prop = obs_properties_add_float_slider(p, k.as_ptr(), text(label), min, max, step);
    if !suffix.is_empty() {
        obs_property_float_set_suffix(prop, suffix.as_ptr());
    }
}

unsafe fn int_list(
    p: *mut obs_properties_t,
    k: &CStr,
    label: &CStr,
    items: &[(&CStr, i64)],
) -> *mut obs_property_t {
    let prop = obs_properties_add_list(
        p,
        k.as_ptr(),
        text(label),
        OBS_COMBO_TYPE_LIST,
        OBS_COMBO_FORMAT_INT,
    );
    for (name, v) in items {
        obs_property_list_add_int(prop, text(name), *v);
    }
    prop
}

const TYPE_LABELS: [(&CStr, BlurType); 4] = [
    (c"Type.Area", BlurType::Area),
    (c"Type.Directional", BlurType::Directional),
    (c"Type.Zoom", BlurType::Zoom),
    (c"Type.TiltShift", BlurType::TiltShift),
];

struct SourceList {
    prop: *mut obs_property_t,
    exclude: *mut obs_source_t,
}

unsafe extern "C" fn add_source(param: *mut c_void, source: *mut obs_source_t) -> bool {
    let list = &*(param as *const SourceList);
    if source == list.exclude || obs_source_get_output_flags(source) & OBS_SOURCE_VIDEO == 0 {
        return true;
    }
    let name = obs_source_get_name(source);
    let uuid = obs_source_get_uuid(source);
    if !name.is_null() && !uuid.is_null() {
        obs_property_list_add_string(list.prop, name, uuid);
    }
    true
}

/// A list of all scenes and video sources (value = UUID).
unsafe fn source_list(
    p: *mut obs_properties_t,
    k: &CStr,
    label: &CStr,
    exclude: *mut obs_source_t,
) -> *mut obs_property_t {
    let prop = obs_properties_add_list(
        p,
        k.as_ptr(),
        text(label),
        OBS_COMBO_TYPE_LIST,
        OBS_COMBO_FORMAT_STRING,
    );
    obs_property_list_add_string(prop, text(c"None"), c"".as_ptr());
    let list = SourceList { prop, exclude };
    let param = &list as *const SourceList as *mut c_void;
    obs_enum_scenes(Some(add_source), param);
    obs_enum_sources(Some(add_source), param);
    prop
}

pub unsafe fn build(parent: *mut obs_source_t) -> *mut obs_properties_t {
    let p = obs_properties_create();

    // --- algorithm -------------------------------------------------------
    let alg = int_list(
        p,
        ALGORITHM,
        c"Algorithm",
        &[
            (c"Algorithm.Gaussian", Algorithm::Gaussian as i64),
            (c"Algorithm.Box", Algorithm::Box as i64),
            (c"Algorithm.DualKawase", Algorithm::DualKawase as i64),
            (c"Algorithm.Pixelate", Algorithm::Pixelate as i64),
        ],
    );
    obs_property_set_modified_callback(alg, Some(on_algorithm_changed));
    let ty = obs_properties_add_list(
        p,
        BLUR_TYPE.as_ptr(),
        text(c"Type"),
        OBS_COMBO_TYPE_LIST,
        OBS_COMBO_FORMAT_INT,
    );
    fill_types(ty, Algorithm::Gaussian);
    obs_property_set_modified_callback(ty, Some(on_changed));

    int_list(
        p,
        PIXEL_SHAPE,
        c"PixelShape",
        &[
            (c"PixelShape.Square", PixelShape::Square as i64),
            (c"PixelShape.Hexagon", PixelShape::Hexagon as i64),
        ],
    );
    slider(p, RADIUS, c"Radius", 0.0, MAX_RADIUS as f64, 0.1, c" px");
    let passes = obs_properties_add_int_slider(p, BOX_PASSES.as_ptr(), text(c"BoxPasses"), 1, 5, 1);
    obs_property_set_long_description(passes, text(c"BoxPasses.Tooltip"));
    slider(p, ANGLE, c"Direction", -180.0, 180.0, 0.1, c"°");

    // --- zoom --------------------------------------------------------------
    let g = obs_properties_create();
    slider(g, ZOOM_X, c"CenterX", 0.0, 100.0, 0.1, c"%");
    slider(g, ZOOM_Y, c"CenterY", 0.0, 100.0, 0.1, c"%");
    slider(g, ZOOM_INACTIVE, c"Zoom.Inactive", 0.0, 100.0, 0.1, c"%");
    obs_properties_add_group(p, G_ZOOM.as_ptr(), text(c"Group.Zoom"), OBS_GROUP_NORMAL, g);

    // --- tilt shift --------------------------------------------------------
    let g = obs_properties_create();
    slider(g, TILT_CENTER, c"Tilt.Center", 0.0, 100.0, 0.1, c"%");
    slider(g, TILT_WIDTH, c"Tilt.Width", 0.0, 100.0, 0.1, c"%");
    slider(g, TILT_FEATHER, c"Tilt.Feather", 0.0, 100.0, 0.1, c"%");
    slider(g, TILT_ANGLE, c"Tilt.Angle", -180.0, 180.0, 0.1, c"°");
    obs_properties_add_group(
        p,
        G_TILT.as_ptr(),
        text(c"Group.TiltShift"),
        OBS_GROUP_NORMAL,
        g,
    );

    // --- background --------------------------------------------------------
    let g = obs_properties_create();
    let bg = source_list(g, BACKGROUND, c"Background.Source", parent);
    obs_property_set_long_description(bg, text(c"Background.Tooltip"));
    obs_property_set_modified_callback(bg, Some(on_changed));
    let mode = int_list(
        g,
        BACKGROUND_MODE,
        c"Background.Mode",
        &[
            (c"Background.Mode.Combined", BackgroundMode::Combined as i64),
            (
                c"Background.Mode.Glass",
                BackgroundMode::FrostedGlass as i64,
            ),
        ],
    );
    obs_property_set_modified_callback(mode, Some(on_changed));
    slider(
        g,
        GLASS_EDGE,
        c"Background.GlassEdge",
        0.1,
        100.0,
        0.1,
        c"%",
    );
    obs_properties_add_group(
        p,
        G_BACKGROUND.as_ptr(),
        text(c"Group.Background"),
        OBS_GROUP_NORMAL,
        g,
    );

    // --- mask ----------------------------------------------------------------
    let g = obs_properties_create();
    let mt = int_list(
        g,
        MASK_TYPE,
        c"Mask.Type",
        &[
            (c"Mask.None", MaskType::None as i64),
            (c"Mask.Crop", MaskType::Crop as i64),
            (c"Mask.Rect", MaskType::Rect as i64),
            (c"Mask.Circle", MaskType::Circle as i64),
            (c"Mask.Source", MaskType::Source as i64),
            (c"Mask.Image", MaskType::Image as i64),
        ],
    );
    obs_property_set_modified_callback(mt, Some(on_changed));
    slider(g, CROP_LEFT, c"Mask.CropLeft", 0.0, 100.0, 0.01, c"%");
    slider(g, CROP_RIGHT, c"Mask.CropRight", 0.0, 100.0, 0.01, c"%");
    slider(g, CROP_TOP, c"Mask.CropTop", 0.0, 100.0, 0.01, c"%");
    slider(g, CROP_BOTTOM, c"Mask.CropBottom", 0.0, 100.0, 0.01, c"%");
    slider(g, RECT_X, c"CenterX", 0.0, 100.0, 0.01, c"%");
    slider(g, RECT_Y, c"CenterY", 0.0, 100.0, 0.01, c"%");
    slider(g, RECT_W, c"Mask.Width", 0.0, 100.0, 0.01, c"%");
    slider(g, RECT_H, c"Mask.Height", 0.0, 100.0, 0.01, c"%");
    slider(g, RECT_ANGLE, c"Mask.Rotation", -180.0, 180.0, 0.1, c"°");
    slider(
        g,
        CORNER_RADIUS,
        c"Mask.CornerRadius",
        0.0,
        1000.0,
        0.5,
        c" px",
    );
    slider(g, CIRCLE_X, c"CenterX", 0.0, 100.0, 0.01, c"%");
    slider(g, CIRCLE_Y, c"CenterY", 0.0, 100.0, 0.01, c"%");
    slider(
        g,
        CIRCLE_RADIUS,
        c"Mask.CircleRadius",
        0.0,
        200.0,
        0.01,
        c"%",
    );
    slider(g, FEATHER, c"Mask.Feather", 0.0, 500.0, 0.5, c" px");
    source_list(g, MASK_SOURCE, c"Mask.SourceSelect", parent);
    let img = cstring(&format!(
        "{} (*.png *.jpg *.jpeg *.bmp *.tga *.gif *.webp)",
        to_string(text(c"Mask.ImageFiles"))
    ));
    obs_properties_add_path(
        g,
        MASK_IMAGE.as_ptr(),
        text(c"Mask.ImageSelect"),
        OBS_PATH_FILE,
        img.as_ptr(),
        std::ptr::null(),
    );
    int_list(
        g,
        MASK_CHANNEL,
        c"Mask.Channel",
        &[
            (c"Mask.Channel.Alpha", MaskChannel::Alpha as i64),
            (c"Mask.Channel.Luminance", MaskChannel::Luminance as i64),
            (c"Mask.Channel.Red", MaskChannel::Red as i64),
            (c"Mask.Channel.Green", MaskChannel::Green as i64),
            (c"Mask.Channel.Blue", MaskChannel::Blue as i64),
        ],
    );
    slider(g, MASK_MULTIPLIER, c"Mask.Multiplier", 0.0, 10.0, 0.01, c"");
    obs_properties_add_bool(g, MASK_INVERT.as_ptr(), text(c"Mask.Invert"));
    obs_properties_add_group(p, G_MASK.as_ptr(), text(c"Group.Mask"), OBS_GROUP_NORMAL, g);

    p
}

unsafe fn fill_types(prop: *mut obs_property_t, alg: Algorithm) {
    obs_property_list_clear(prop);
    for (label, t) in TYPE_LABELS {
        if alg.supported_types().contains(&t) {
            obs_property_list_add_int(prop, text(label), t as i64);
        }
    }
}

unsafe extern "C" fn on_algorithm_changed(
    props: *mut obs_properties_t,
    _p: *mut obs_property_t,
    settings: *mut obs_data_t,
) -> bool {
    let alg = Algorithm::from_i64(Data(settings).int(ALGORITHM));
    fill_types(obs_properties_get(props, BLUR_TYPE.as_ptr()), alg);
    on_changed(props, _p, settings)
}

/// Show only the settings relevant for the current selection.
unsafe extern "C" fn on_changed(
    props: *mut obs_properties_t,
    _p: *mut obs_property_t,
    settings: *mut obs_data_t,
) -> bool {
    let d = Data(settings);
    let alg = Algorithm::from_i64(d.int(ALGORITHM));
    let ty = alg.effective_type(BlurType::from_i64(d.int(BLUR_TYPE)));
    let mask = MaskType::from_i64(d.int(MASK_TYPE));
    let has_bg = !d.string(BACKGROUND).is_empty();
    let glass = BackgroundMode::from_i64(d.int(BACKGROUND_MODE)) == BackgroundMode::FrostedGlass;

    let vis = |k: &CStr, v: bool| {
        let p = obs_properties_get(props, k.as_ptr());
        if !p.is_null() {
            obs_property_set_visible(p, v);
        }
    };
    let separable = matches!(alg, Algorithm::Gaussian | Algorithm::Box);
    vis(BLUR_TYPE, alg != Algorithm::Pixelate);
    vis(PIXEL_SHAPE, alg == Algorithm::Pixelate);
    vis(BOX_PASSES, alg == Algorithm::Box);
    vis(ANGLE, separable && ty == BlurType::Directional);
    vis(G_ZOOM, separable && ty == BlurType::Zoom);
    vis(G_TILT, ty == BlurType::TiltShift);
    let radius = obs_properties_get(props, RADIUS.as_ptr());
    if !radius.is_null() {
        let label = if alg == Algorithm::Pixelate {
            c"PixelSize"
        } else {
            c"Radius"
        };
        obs_property_set_description(radius, text(label));
    }

    vis(BACKGROUND_MODE, has_bg);
    vis(GLASS_EDGE, has_bg && glass);

    let geo = matches!(mask, MaskType::Crop | MaskType::Rect | MaskType::Circle);
    for k in [CROP_LEFT, CROP_RIGHT, CROP_TOP, CROP_BOTTOM] {
        vis(k, mask == MaskType::Crop);
    }
    for k in [RECT_X, RECT_Y, RECT_W, RECT_H, RECT_ANGLE] {
        vis(k, mask == MaskType::Rect);
    }
    vis(
        CORNER_RADIUS,
        matches!(mask, MaskType::Crop | MaskType::Rect),
    );
    for k in [CIRCLE_X, CIRCLE_Y, CIRCLE_RADIUS] {
        vis(k, mask == MaskType::Circle);
    }
    vis(FEATHER, geo);
    vis(MASK_SOURCE, mask == MaskType::Source);
    vis(MASK_IMAGE, mask == MaskType::Image);
    let tex = matches!(mask, MaskType::Source | MaskType::Image);
    vis(MASK_CHANNEL, tex);
    vis(MASK_MULTIPLIER, tex);
    vis(MASK_INVERT, mask != MaskType::None);
    true
}
