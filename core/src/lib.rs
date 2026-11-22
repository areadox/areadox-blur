//! Pure, OBS-independent logic of the Areadox Blur filter: setting enums,
//! derived render parameters and the pass planning for each algorithm.
//! Everything in here is unit-tested without libobs.

/// Blur algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Algorithm {
    #[default]
    Gaussian = 0,
    Box = 1,
    DualKawase = 2,
    Pixelate = 3,
}

/// Shape of the blur for Gaussian/Box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlurType {
    #[default]
    Area = 0,
    Directional = 1,
    Zoom = 2,
    TiltShift = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PixelShape {
    #[default]
    Square = 0,
    Hexagon = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MaskType {
    #[default]
    None = 0,
    Crop = 1,
    Rect = 2,
    Circle = 3,
    Source = 4,
    Image = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MaskChannel {
    #[default]
    Alpha = 0,
    Luminance = 1,
    Red = 2,
    Green = 3,
    Blue = 4,
}

/// How a background source is combined with the filtered source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackgroundMode {
    /// Source is drawn over the background, then both are blurred together
    /// (behaviour of Composite Blur's "background source").
    #[default]
    Combined = 0,
    /// Only the background is blurred and shows through wherever the source
    /// has alpha coverage; the source itself stays sharp on top ("frosted glass").
    FrostedGlass = 1,
}

macro_rules! from_int {
    ($t:ty { $($v:ident),* }) => {
        impl $t {
            pub fn from_i64(v: i64) -> Self {
                $( if v == <$t>::$v as i64 { return <$t>::$v; } )*
                <$t>::default()
            }
        }
    };
}
from_int!(Algorithm {
    Gaussian,
    Box,
    DualKawase,
    Pixelate
});
from_int!(BlurType {
    Area,
    Directional,
    Zoom,
    TiltShift
});
from_int!(PixelShape { Square, Hexagon });
from_int!(MaskType {
    None,
    Crop,
    Rect,
    Circle,
    Source,
    Image
});
from_int!(MaskChannel {
    Alpha,
    Luminance,
    Red,
    Green,
    Blue
});
from_int!(BackgroundMode {
    Combined,
    FrostedGlass
});

impl Algorithm {
    /// Blur types offered for this algorithm in the UI.
    pub fn supported_types(self) -> &'static [BlurType] {
        match self {
            Algorithm::Gaussian | Algorithm::Box => &[
                BlurType::Area,
                BlurType::Directional,
                BlurType::Zoom,
                BlurType::TiltShift,
            ],
            Algorithm::DualKawase => &[BlurType::Area, BlurType::TiltShift],
            Algorithm::Pixelate => &[BlurType::Area],
        }
    }

    /// The blur type that is actually rendered (unsupported types fall back to Area).
    pub fn effective_type(self, t: BlurType) -> BlurType {
        if self.supported_types().contains(&t) {
            t
        } else {
            BlurType::Area
        }
    }
}

/// All user settings of one filter instance, already range-checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub algorithm: Algorithm,
    pub blur_type: BlurType,
    /// Blur radius in pixels (pixel size for Pixelate).
    pub radius: f32,
    /// Number of box passes (1..=5); 3 passes approximate a Gaussian.
    pub box_passes: u32,
    /// Direction in degrees for directional blur.
    pub angle: f32,
    pub pixel_shape: PixelShape,
    /// Zoom center in 0..1 (relative to the source size).
    pub zoom_center: (f32, f32),
    /// Fraction (0..1) of the distance to the farthest corner that stays sharp.
    pub zoom_inactive: f32,
    /// Tilt-shift: band center (0..1), band height (0..1 of source height),
    /// transition height (0..1) and rotation in degrees.
    pub tilt_center: f32,
    pub tilt_width: f32,
    pub tilt_feather: f32,
    pub tilt_angle: f32,

    pub background_uuid: String,
    pub background_mode: BackgroundMode,
    /// Alpha value at which a pixel counts as fully covered by "glass".
    pub glass_edge: f32,

    pub mask: MaskType,
    pub mask_invert: bool,
    /// Crop mask in percent of the source size: left, right, top, bottom.
    pub crop: [f32; 4],
    /// Rectangle mask: center (0..1), size (0..1), rotation in degrees.
    pub rect_center: (f32, f32),
    pub rect_size: (f32, f32),
    pub rect_angle: f32,
    /// Corner radius in pixels (crop + rect).
    pub corner_radius: f32,
    /// Circle mask center (0..1) and radius (fraction of the smaller side).
    pub circle_center: (f32, f32),
    pub circle_radius: f32,
    /// Edge softness of geometric masks in pixels.
    pub feather: f32,
    pub mask_source_uuid: String,
    pub mask_image_path: String,
    pub mask_channel: MaskChannel,
    pub mask_multiplier: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            algorithm: Algorithm::Gaussian,
            blur_type: BlurType::Area,
            radius: 10.0,
            box_passes: 3,
            angle: 0.0,
            pixel_shape: PixelShape::Square,
            zoom_center: (0.5, 0.5),
            zoom_inactive: 0.0,
            tilt_center: 0.5,
            tilt_width: 0.25,
            tilt_feather: 0.2,
            tilt_angle: 0.0,
            background_uuid: String::new(),
            background_mode: BackgroundMode::Combined,
            glass_edge: 0.05,
            mask: MaskType::None,
            mask_invert: false,
            crop: [0.0; 4],
            rect_center: (0.5, 0.5),
            rect_size: (0.5, 0.5),
            rect_angle: 0.0,
            corner_radius: 0.0,
            circle_center: (0.5, 0.5),
            circle_radius: 0.25,
            feather: 2.0,
            mask_source_uuid: String::new(),
            mask_image_path: String::new(),
            mask_channel: MaskChannel::Alpha,
            mask_multiplier: 1.0,
        }
    }
}

impl Settings {
    /// Clamp every value into its valid range (protects the shaders from
    /// nonsense coming from hand-edited scene collections).
    pub fn sanitized(mut self) -> Self {
        fn fin(v: f32, d: f32) -> f32 {
            if v.is_finite() {
                v
            } else {
                d
            }
        }
        let d = Settings::default();
        self.radius = fin(self.radius, d.radius).clamp(0.0, MAX_RADIUS);
        self.box_passes = self.box_passes.clamp(1, 5);
        self.angle = fin(self.angle, 0.0);
        let c01 = |v: f32, d: f32| fin(v, d).clamp(0.0, 1.0);
        self.zoom_center = (c01(self.zoom_center.0, 0.5), c01(self.zoom_center.1, 0.5));
        self.zoom_inactive = c01(self.zoom_inactive, 0.0);
        self.tilt_center = c01(self.tilt_center, 0.5);
        self.tilt_width = c01(self.tilt_width, 0.25);
        self.tilt_feather = c01(self.tilt_feather, 0.2);
        self.tilt_angle = fin(self.tilt_angle, 0.0);
        self.glass_edge = fin(self.glass_edge, d.glass_edge).clamp(0.001, 1.0);
        for v in &mut self.crop {
            *v = fin(*v, 0.0).clamp(0.0, 100.0);
        }
        self.rect_center = (c01(self.rect_center.0, 0.5), c01(self.rect_center.1, 0.5));
        self.rect_size = (c01(self.rect_size.0, 0.5), c01(self.rect_size.1, 0.5));
        self.rect_angle = fin(self.rect_angle, 0.0);
        self.corner_radius = fin(self.corner_radius, 0.0).max(0.0);
        self.circle_center = (
            c01(self.circle_center.0, 0.5),
            c01(self.circle_center.1, 0.5),
        );
        self.circle_radius = fin(self.circle_radius, 0.25).clamp(0.0, 2.0);
        self.feather = fin(self.feather, 2.0).max(0.0);
        self.mask_multiplier = fin(self.mask_multiplier, 1.0).clamp(0.0, 100.0);
        self
    }
}

pub const MAX_RADIUS: f32 = 500.0;

/// Largest kernel radius (in pixels of the working resolution) a single
/// Gaussian/Box pass handles. Bigger radii are rendered on a downsampled
/// copy, which keeps the cost per pixel bounded.
pub const MAX_PASS_RADIUS: f32 = 24.0;

/// How a separable (Gaussian/Box) blur is rendered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeparablePlan {
    /// Number of 2x downsample steps before blurring.
    pub levels: u32,
    /// Kernel radius per pass in pixels of the downsampled image.
    pub pass_radius: f32,
    /// How many horizontal+vertical pass pairs (Box blur iterations).
    pub iterations: u32,
}

/// Plan a separable blur. For box blur with `n` iterations the per-pass radius
/// is chosen so the combined standard deviation matches a single box of the
/// requested radius scaled by sqrt(n) – i.e. more passes = smoother, same size.
pub fn plan_separable(radius: f32, iterations: u32, min_dim: u32) -> SeparablePlan {
    let iterations = iterations.max(1);
    let per_iter = if iterations > 1 {
        radius / (iterations as f32).sqrt()
    } else {
        radius
    };
    let mut levels = 0u32;
    let mut r = per_iter;
    // Never downsample below ~16 px on the smaller side.
    while r > MAX_PASS_RADIUS && (min_dim >> (levels + 1)) >= 16 {
        levels += 1;
        r *= 0.5;
    }
    SeparablePlan {
        levels,
        pass_radius: r.min(MAX_PASS_RADIUS * 2.0),
        iterations,
    }
}

/// Plan for the Dual Kawase blur: number of down/up iterations and the
/// sample offset (in source texels) that approximates `radius`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KawasePlan {
    pub iterations: u32,
    pub offset: f32,
}

/// Effective blur radius in pixels of a dual-kawase chain with the given
/// iteration count and offset. Measured on a step edge in libobs: the chain
/// has a standard deviation of about `2^n * offset * 1.5` px, and "radius"
/// means 2 sigma everywhere in this plugin.
pub fn kawase_radius(iterations: u32, offset: f32) -> f32 {
    (1u32 << iterations) as f32 * offset * KAWASE_SCALE
}

/// See [`kawase_radius`].
pub const KAWASE_SCALE: f32 = 3.0;

/// Kernel radius handed to the Gaussian/Box shaders so that every algorithm
/// produces the same amount of blur for the same user radius: the result has
/// a standard deviation of `radius / 2`.
/// * Gaussian: the shader uses sigma = kernel / 3  -> kernel = 1.5 * radius
/// * Box:      a box of half-width k has sigma = k / sqrt(3) -> k = radius * sqrt(3) / 2
pub fn shader_kernel_radius(algorithm: Algorithm, radius: f32) -> f32 {
    match algorithm {
        Algorithm::Box => radius * 0.866_025_4,
        _ => radius * 1.5,
    }
}

pub fn plan_kawase(radius: f32, min_dim: u32) -> Option<KawasePlan> {
    if radius < 0.5 {
        return None;
    }
    let mut iterations = 1u32;
    // Choose the smallest iteration count whose maximum (offset 2) reaches the radius.
    while kawase_radius(iterations, 2.0) < radius
        && iterations < 8
        && (min_dim >> (iterations + 1)) >= 4
    {
        iterations += 1;
    }
    let offset = (radius / ((1u32 << iterations) as f32 * KAWASE_SCALE)).clamp(0.25, 3.0);
    Some(KawasePlan { iterations, offset })
}

/// Size of downsample level `level` (level 0 = full size), never below 1 px.
pub fn level_size(w: u32, h: u32, level: u32) -> (u32, u32) {
    ((w >> level).max(1), (h >> level).max(1))
}

/// Geometry of a (rotated, rounded) rectangle in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectPx {
    pub center: (f32, f32),
    pub half_size: (f32, f32),
    pub angle_rad: f32,
    pub corner: f32,
}

/// Convert crop percentages (left, right, top, bottom) to a rectangle.
pub fn crop_rect(crop: [f32; 4], corner: f32, w: f32, h: f32) -> RectPx {
    let l = crop[0] / 100.0 * w;
    let r = w - crop[1] / 100.0 * w;
    let t = crop[2] / 100.0 * h;
    let b = h - crop[3] / 100.0 * h;
    let (r, b) = (r.max(l), b.max(t));
    RectPx {
        center: ((l + r) * 0.5, (t + b) * 0.5),
        half_size: ((r - l) * 0.5, (b - t) * 0.5),
        angle_rad: 0.0,
        corner: corner.min((r - l) * 0.5).min((b - t) * 0.5).max(0.0),
    }
}

pub fn rect_from_settings(s: &Settings, w: f32, h: f32) -> RectPx {
    let half = (s.rect_size.0 * w * 0.5, s.rect_size.1 * h * 0.5);
    RectPx {
        center: (s.rect_center.0 * w, s.rect_center.1 * h),
        half_size: half,
        angle_rad: s.rect_angle.to_radians(),
        corner: s.corner_radius.min(half.0).min(half.1).max(0.0),
    }
}

/// Unit direction vector in texture space (y down) for an angle in degrees.
pub fn direction(angle_deg: f32) -> (f32, f32) {
    let a = angle_deg.to_radians();
    (a.cos(), a.sin())
}

/// RGBA weights used by the texture mask shader to pick a channel.
/// Luminance is flagged separately (returns zeros here).
pub fn channel_weights(c: MaskChannel) -> [f32; 4] {
    match c {
        MaskChannel::Alpha => [0.0, 0.0, 0.0, 1.0],
        MaskChannel::Red => [1.0, 0.0, 0.0, 0.0],
        MaskChannel::Green => [0.0, 1.0, 0.0, 0.0],
        MaskChannel::Blue => [0.0, 0.0, 1.0, 0.0],
        MaskChannel::Luminance => [0.0; 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_roundtrip_and_fallback() {
        assert_eq!(Algorithm::from_i64(2), Algorithm::DualKawase);
        assert_eq!(Algorithm::from_i64(99), Algorithm::Gaussian);
        assert_eq!(MaskType::from_i64(5), MaskType::Image);
        assert_eq!(BlurType::from_i64(-1), BlurType::Area);
    }

    #[test]
    fn effective_type_falls_back() {
        assert_eq!(
            Algorithm::Pixelate.effective_type(BlurType::Zoom),
            BlurType::Area
        );
        assert_eq!(
            Algorithm::Box.effective_type(BlurType::Zoom),
            BlurType::Zoom
        );
        assert_eq!(
            Algorithm::DualKawase.effective_type(BlurType::TiltShift),
            BlurType::TiltShift
        );
    }

    #[test]
    fn separable_small_radius_no_downsample() {
        let p = plan_separable(10.0, 1, 1080);
        assert_eq!(p.levels, 0);
        assert_eq!(p.pass_radius, 10.0);
    }

    #[test]
    fn separable_large_radius_downsamples() {
        let p = plan_separable(500.0, 1, 1080);
        assert!(p.levels >= 4, "{p:?}");
        assert!(p.pass_radius <= MAX_PASS_RADIUS);
        // radius is preserved: pass_radius * 2^levels == 500
        assert!((p.pass_radius * (1 << p.levels) as f32 - 500.0).abs() < 1e-3);
    }

    #[test]
    fn separable_respects_tiny_sources() {
        let p = plan_separable(500.0, 1, 40);
        assert!(p.levels <= 1, "{p:?}");
    }

    #[test]
    fn box_iterations_split_radius() {
        let p = plan_separable(30.0, 4, 1080);
        assert_eq!(p.iterations, 4);
        assert!((p.pass_radius - 15.0).abs() < 1e-4);
    }

    #[test]
    fn kawase_monotonic() {
        assert!(plan_kawase(0.1, 1080).is_none());
        let mut last = 0.0;
        for r in [1.0f32, 5.0, 10.0, 30.0, 80.0, 200.0, 500.0] {
            let p = plan_kawase(r, 1080).unwrap();
            let eff = kawase_radius(p.iterations, p.offset);
            assert!(eff >= last, "r={r} {p:?}");
            last = eff;
            if r >= 3.0 {
                assert!((eff - r).abs() / r < 0.05, "r={r} eff={eff} {p:?}");
            }
        }
    }

    #[test]
    fn kernel_radius_matches_sigma() {
        // gaussian: sigma = kernel / 3 must equal radius / 2
        assert!((shader_kernel_radius(Algorithm::Gaussian, 20.0) / 3.0 - 10.0).abs() < 1e-4);
        // box: sigma = k / sqrt(3)
        assert!((shader_kernel_radius(Algorithm::Box, 20.0) / 3f32.sqrt() - 10.0).abs() < 1e-3);
    }

    #[test]
    fn crop_rect_geometry() {
        let r = crop_rect([10.0, 10.0, 0.0, 50.0], 1000.0, 1000.0, 500.0);
        assert_eq!(r.center, (500.0, 125.0));
        assert_eq!(r.half_size, (400.0, 125.0));
        assert_eq!(r.corner, 125.0);
        // Overlapping crops collapse instead of inverting
        let r = crop_rect([80.0, 80.0, 0.0, 0.0], 0.0, 100.0, 100.0);
        assert_eq!(r.half_size.0, 0.0);
    }

    #[test]
    fn sanitize_clamps() {
        let s = Settings {
            radius: f32::NAN,
            box_passes: 0,
            crop: [-5.0, 200.0, 1.0, 1.0],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.radius, 10.0);
        assert_eq!(s.box_passes, 1);
        assert_eq!(s.crop[0], 0.0);
        assert_eq!(s.crop[1], 100.0);
    }

    #[test]
    fn direction_is_unit() {
        let (x, y) = direction(90.0);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
    }
}
