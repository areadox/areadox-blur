// Headless libobs 33 test harness for the areadox-blur plugin.
// Loads the plugin, applies the filter with many setting combinations to a
// synthetic test source and writes the rendered frames as PPM (RGBA->RGB over
// a checkerboard so transparency is visible) plus raw alpha stats.
#include <obs.h>
#include <obs-nix-platform.h>
#include <obs-module.h>
#include <util/base.h>
#include <util/bmem.h>
#include <graphics/graphics.h>
#include <X11/Xlib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <stdarg.h>

#define W 640
#define H 360

static int errors = 0;
static void log_handler(int lvl, const char *fmt, va_list args, void *p)
{
	char buf[4096];
	vsnprintf(buf, sizeof(buf), fmt, args);
	if (lvl <= LOG_WARNING)
		errors++;
	if (lvl <= LOG_INFO || strstr(buf, "areadox"))
		printf("[%d] %s\n", lvl, buf);
}

// ---- synthetic test source ------------------------------------------------
struct test_src { gs_texture_t *tex; int kind; };

static void make_pattern(uint8_t *px, int kind)
{
	for (int y = 0; y < H; y++)
		for (int x = 0; x < W; x++) {
			uint8_t *p = px + (y * W + x) * 4;
			if (kind == 2) {
				// clean vertical step edge for calibration
				p[0] = p[1] = p[2] = 255; p[3] = x < W / 2 ? 0 : 255;
			} else if (kind == 0) {
				// foreground: colour bars + white grid lines, transparent left quarter,
				// half transparent stripe in the middle
				int bar = x / 80;
				static const uint8_t cols[8][3] = {{255,0,0},{0,255,0},{0,0,255},{255,255,0},{0,255,255},{255,0,255},{255,255,255},{20,20,20}};
				p[0] = cols[bar][0]; p[1] = cols[bar][1]; p[2] = cols[bar][2]; p[3] = 255;
				if (x % 40 == 0 || y % 40 == 0) { p[0] = p[1] = p[2] = 255; }
				if (x < W / 4) p[3] = 0;
				if (y > 150 && y < 210) p[3] = 128;
			} else {
				// background: diagonal stripes
				int s = ((x + y) / 20) % 2;
				p[0] = s ? 250 : 30; p[1] = s ? 120 : 30; p[2] = s ? 20 : 90; p[3] = 255;
			}
		}
}

static const char *ts_name(void *u) { (void)u; return "Test"; }
static void *ts_create(obs_data_t *s, obs_source_t *src)
{
	(void)src;
	struct test_src *t = bzalloc(sizeof(*t));
	t->kind = (int)obs_data_get_int(s, "kind");
	uint8_t *px = bmalloc(W * H * 4);
	make_pattern(px, t->kind);
	obs_enter_graphics();
	const uint8_t *planes[1] = {px};
	t->tex = gs_texture_create(W, H, GS_RGBA, 1, planes, 0);
	obs_leave_graphics();
	bfree(px);
	return t;
}
static void ts_destroy(void *d) { struct test_src *t = d; obs_enter_graphics(); gs_texture_destroy(t->tex); obs_leave_graphics(); bfree(t); }
static uint32_t ts_w(void *d) { (void)d; return W; }
static uint32_t ts_h(void *d) { (void)d; return H; }
static void ts_render(void *d, gs_effect_t *effect)
{
	struct test_src *t = d;
	(void)effect;
	gs_effect_t *e = obs_get_base_effect(OBS_EFFECT_DEFAULT);
	const bool prev = gs_framebuffer_srgb_enabled();
	gs_enable_framebuffer_srgb(true);
	gs_effect_set_texture_srgb(gs_effect_get_param_by_name(e, "image"), t->tex);
	while (gs_effect_loop(e, "Draw"))
		gs_draw_sprite(t->tex, 0, W, H);
	gs_enable_framebuffer_srgb(prev);
}

static struct obs_source_info test_info = {
	.id = "test_pattern", .type = OBS_SOURCE_TYPE_INPUT, .output_flags = OBS_SOURCE_VIDEO | OBS_SOURCE_SRGB | OBS_SOURCE_CUSTOM_DRAW,
	.get_name = ts_name, .create = ts_create, .destroy = ts_destroy, .get_width = ts_w, .get_height = ts_h,
	.video_render = ts_render,
};

// ---- capture ---------------------------------------------------------------
static void capture(obs_source_t *src, const char *path, double *alpha_mean)
{
	uint8_t *data; uint32_t linesize;
	obs_enter_graphics();
	gs_texrender_t *tr = gs_texrender_create(GS_RGBA, GS_ZS_NONE);
	gs_stagesurf_t *st = gs_stagesurface_create(W, H, GS_RGBA);
	if (gs_texrender_begin(tr, W, H)) {
		struct vec4 z; vec4_zero(&z);
		gs_clear(GS_CLEAR_COLOR, &z, 0, 0);
		gs_ortho(0, W, 0, H, -100, 100);
		gs_blend_state_push();
		gs_blend_function_separate(GS_BLEND_SRCALPHA, GS_BLEND_INVSRCALPHA, GS_BLEND_ONE, GS_BLEND_INVSRCALPHA);
		obs_source_video_render(src);
		gs_blend_state_pop();
		gs_texrender_end(tr);
	}
	gs_stage_texture(st, gs_texrender_get_texture(tr));
	FILE *f = fopen(path, "wb");
	double asum = 0;
	if (gs_stagesurface_map(st, &data, &linesize)) {
		fprintf(f, "P6\n%d %d\n255\n", W, H);
		for (int y = 0; y < H; y++)
			for (int x = 0; x < W; x++) {
				uint8_t *p = data + y * linesize + x * 4;
				// texture is premultiplied (blend into empty target); composite over checkerboard
				int chk = ((x / 16 + y / 16) % 2) ? 200 : 120;
				uint8_t rgb[3];
				for (int c = 0; c < 3; c++) {
					int v = p[c] + chk * (255 - p[3]) / 255;
					rgb[c] = v > 255 ? 255 : v;
				}
				asum += p[3];
				fwrite(rgb, 1, 3, f);
			}
		char apath[1100];
		snprintf(apath, sizeof(apath), "%s.alpha", path);
		FILE *fa = fopen(apath, "wb");
		if (fa) {
			for (int y = 0; y < H; y++)
				for (int x = 0; x < W; x++)
					fputc(data[y * linesize + x * 4 + 3], fa);
			fclose(fa);
		}
		gs_stagesurface_unmap(st);
	}
	fclose(f);
	gs_stagesurface_destroy(st);
	gs_texrender_destroy(tr);
	obs_leave_graphics();
	*alpha_mean = asum / (W * H);
}

int main(int argc, char **argv)
{
	if (argc < 4) { fprintf(stderr, "usage: %s plugin.so data_dir out_dir\n", argv[0]); return 2; }
	setvbuf(stdout, NULL, _IONBF, 0);
	base_set_log_handler(log_handler, NULL);
	Display *dpy = XOpenDisplay(NULL);
	if (!dpy) { fprintf(stderr, "no X display\n"); return 1; }
	obs_set_nix_platform(OBS_NIX_PLATFORM_X11_EGL);
	obs_set_nix_platform_display(dpy);
	// libobs' own effect files (obs-studio/libobs/data/)
	obs_add_data_path(getenv("LIBOBS_DATA") ? getenv("LIBOBS_DATA") : "/usr/share/obs/libobs/");
	if (!obs_startup("de-DE", NULL, NULL)) { fprintf(stderr, "startup failed\n"); return 1; }

	struct obs_video_info ovi = {
		.graphics_module = "libobs-opengl", .fps_num = 30, .fps_den = 1,
		.base_width = W, .base_height = H, .output_width = W, .output_height = H,
		.output_format = VIDEO_FORMAT_NV12, .adapter = 0, .gpu_conversion = true,
		.colorspace = VIDEO_CS_709, .range = VIDEO_RANGE_PARTIAL, .scale_type = OBS_SCALE_BICUBIC,
	};
	int r = obs_reset_video(&ovi);
	if (r != OBS_VIDEO_SUCCESS) { fprintf(stderr, "reset_video failed %d\n", r); return 1; }

	obs_register_source(&test_info);

	obs_module_t *mod = NULL;
	r = obs_open_module(&mod, argv[1], argv[2]);
	if (r != MODULE_SUCCESS) { fprintf(stderr, "open_module failed %d\n", r); return 1; }
	if (!obs_init_module(mod)) { fprintf(stderr, "init_module failed\n"); return 1; }
	printf("module name: %s | description: %s\n", obs_get_module_name(mod), obs_get_module_description(mod));
	printf("filter display name: %s\n", obs_source_get_display_name("areadox_blur_filter"));

	obs_data_t *s = obs_data_create();
	obs_data_set_int(s, "kind", getenv("FG_KIND") ? atoi(getenv("FG_KIND")) : 0);
	obs_source_t *fg = obs_source_create("test_pattern", "fg", s, NULL);
	obs_data_set_int(s, "kind", 1);
	obs_source_t *bg = obs_source_create("test_pattern", "bg", s, NULL);
	obs_data_release(s);

	obs_source_t *filter = obs_source_create("areadox_blur_filter", "blur", NULL, NULL);
	if (!filter) { fprintf(stderr, "filter create failed\n"); return 1; }
	obs_source_filter_add(fg, filter);

	// properties smoke test (exercises list building + modified callbacks)
	obs_properties_t *props = obs_source_properties(filter);
	int nprops = 0;
	for (obs_property_t *p = obs_properties_first(props); p; obs_property_next(&p)) nprops++;
	obs_data_t *cur = obs_source_get_settings(filter);
	obs_property_t *alg = obs_properties_get(props, "blur_algorithm");
	obs_property_modified(alg, cur);
	printf("top-level properties: %d, background list items: %zu\n", nprops,
	       obs_property_list_item_count(obs_properties_get(props, "background")));
	obs_data_release(cur);
	obs_properties_destroy(props);

	// write a mask image (radial gradient) via ffmpeg-free path: PPM is not
	// supported by libobs, so the caller provides mask.png in out_dir.
	char mask_png[1024];
	snprintf(mask_png, sizeof(mask_png), "%s/mask.png", argv[3]);

	const char *bg_uuid = obs_source_get_uuid(bg);
	const char *fg_name = "fg";
	(void)fg_name;

	struct { const char *name; const char *json; } cases[] = {
		{"00_none", "{\"radius\":0}"},
		{"01_gauss_area_10", "{\"blur_algorithm\":0,\"radius\":10}"},
		{"02_gauss_area_80", "{\"blur_algorithm\":0,\"radius\":80}"},
		{"03_gauss_area_400", "{\"blur_algorithm\":0,\"radius\":400}"},
		{"04_box_area_10_p3", "{\"blur_algorithm\":1,\"radius\":10,\"box_passes\":3}"},
		{"05_box_area_30_p1", "{\"blur_algorithm\":1,\"radius\":30,\"box_passes\":1}"},
		{"06_kawase_10", "{\"blur_algorithm\":2,\"radius\":10}"},
		{"07_kawase_80", "{\"blur_algorithm\":2,\"radius\":80}"},
		{"08_pixel_sq_20", "{\"blur_algorithm\":3,\"radius\":20,\"pixelate_shape\":0}"},
		{"09_pixel_hex_20", "{\"blur_algorithm\":3,\"radius\":20,\"pixelate_shape\":1}"},
		{"10_gauss_dir_40_45", "{\"blur_algorithm\":0,\"blur_type\":1,\"radius\":40,\"direction\":45}"},
		{"11_box_dir_40_0", "{\"blur_algorithm\":1,\"blur_type\":1,\"radius\":40,\"direction\":0,\"box_passes\":2}"},
		{"12_gauss_zoom_40", "{\"blur_algorithm\":0,\"blur_type\":2,\"radius\":40,\"zoom_center_x\":60,\"zoom_center_y\":50,\"zoom_inactive_radius\":10}"},
		{"13_gauss_tilt_20", "{\"blur_algorithm\":0,\"blur_type\":3,\"radius\":20,\"tilt_center\":50,\"tilt_width\":20,\"tilt_feather\":20,\"tilt_angle\":10}"},
		{"14_kawase_tilt_20", "{\"blur_algorithm\":2,\"blur_type\":3,\"radius\":20}"},
		{"15_mask_crop", "{\"radius\":15,\"mask_type\":1,\"mask_crop_left\":40,\"mask_crop_right\":10,\"mask_crop_top\":20,\"mask_crop_bottom\":20,\"mask_corner_radius\":30}"},
		{"16_mask_rect_rot", "{\"radius\":15,\"mask_type\":2,\"mask_rect_width\":40,\"mask_rect_height\":40,\"mask_rect_angle\":20,\"mask_corner_radius\":20,\"mask_feather\":10}"},
		{"17_mask_circle_inv", "{\"radius\":15,\"mask_type\":3,\"mask_circle_radius\":30,\"mask_feather\":20,\"mask_invert\":true}"},
		{"18_mask_source_luma", "MASKSRC"},
		{"19_mask_image", "MASKIMG"},
		{"20_bg_combined", "BGCOMB"},
		{"21_bg_glass", "BGGLASS"},
		{"22_bg_glass_rect", "BGGLASSRECT"},
	};

	int failures = 0;
	size_t ncases = sizeof(cases) / sizeof(cases[0]);
	if (getenv("CASES")) {
		// CASES="name=json;name=json"
		static char buf[65536];
		snprintf(buf, sizeof(buf), "%s", getenv("CASES"));
		ncases = 0;
		for (char *tok = strtok(buf, ";"); tok && ncases < 23; tok = strtok(NULL, ";")) {
			char *eq = strchr(tok, '=');
			if (!eq) continue;
			*eq = 0;
			cases[ncases].name = tok;
			cases[ncases].json = eq + 1;
			ncases++;
		}
	}
	for (size_t i = 0; i < ncases; i++) {
		obs_data_t *d;
		char json[2048];
		if (!strcmp(cases[i].json, "MASKSRC"))
			snprintf(json, sizeof(json), "{\"radius\":15,\"mask_type\":4,\"mask_channel\":1,\"mask_source\":\"%s\"}", bg_uuid);
		else if (!strcmp(cases[i].json, "MASKIMG"))
			snprintf(json, sizeof(json), "{\"radius\":15,\"mask_type\":5,\"mask_channel\":1,\"mask_image\":\"%s\"}", mask_png);
		else if (!strcmp(cases[i].json, "BGCOMB"))
			snprintf(json, sizeof(json), "{\"radius\":12,\"background\":\"%s\",\"background_mode\":0}", bg_uuid);
		else if (!strcmp(cases[i].json, "BGGLASS"))
			snprintf(json, sizeof(json), "{\"radius\":12,\"background\":\"%s\",\"background_mode\":1}", bg_uuid);
		else if (!strcmp(cases[i].json, "BGGLASSRECT"))
			snprintf(json, sizeof(json), "{\"radius\":12,\"background\":\"%s\",\"background_mode\":0,\"mask_type\":2,\"mask_rect_width\":50,\"mask_rect_height\":60,\"mask_corner_radius\":25}", bg_uuid);
		else
			snprintf(json, sizeof(json), "%s", cases[i].json);
		d = obs_data_create_from_json(json);
		// reset to defaults first so cases are independent
		obs_data_t *defaults = obs_data_create();
		obs_source_update(filter, defaults);
		obs_data_release(defaults);
		obs_data_t *fresh = obs_source_get_settings(filter);
		obs_data_clear(fresh);
		obs_data_apply(fresh, d);
		obs_source_update(filter, fresh);
		obs_data_release(fresh);
		obs_data_release(d);
		usleep(80 * 1000); // let the video thread tick (resets the per-frame cache)
		char path[1024];
		snprintf(path, sizeof(path), "%s/%s.ppm", argv[3], cases[i].name);
		double am;
		capture(fg, path, &am);
		printf("case %-22s alpha_mean=%6.1f\n", cases[i].name, am);
		if (am < 1.0) failures++;
	}

	// scene test: put fg into a scene shown on output channel 0, use the
	// scene itself as background (recursion guard) and the bg as mask source.
	obs_scene_t *scene = getenv("SKIP_SCENE") ? NULL : obs_scene_create("scene");
	obs_source_t *scene_src = scene ? obs_scene_get_source(scene) : NULL;
	if (scene) {
	obs_scene_add(scene, fg);
	obs_set_output_source(0, scene_src);
	{
		char json[1024];
		snprintf(json, sizeof(json), "{\"radius\":20,\"background\":\"%s\",\"mask_type\":4,\"mask_source\":\"%s\"}",
			 obs_source_get_uuid(scene_src), bg_uuid);
		obs_data_t *d = obs_data_create_from_json(json);
		obs_source_update(filter, d);
		obs_data_release(d);
		usleep(300 * 1000);
		double am;
		char spath[1024];
		snprintf(spath, sizeof(spath), "%s/30_scene_recursive.ppm", argv[3]);
		capture(scene_src, spath, &am);
		printf("scene recursive alpha_mean=%.1f showing(bg)=%d\n", am, obs_source_showing(bg));
	}
	obs_set_output_source(0, NULL);
	usleep(100 * 1000);
	printf("after hide: showing(bg)=%d\n", obs_source_showing(bg));
	}

	// cached second render in the same frame must work too
	double am1, am2;
	capture(fg, "/dev/null", &am1);
	capture(fg, "/dev/null", &am2);
	printf("double-render alpha: %.1f / %.1f\n", am1, am2);

	if (scene) obs_scene_release(scene);
	usleep(200 * 1000);
	obs_source_filter_remove(fg, filter);
	obs_source_release(filter);
	obs_source_release(fg);
	obs_source_release(bg);
	usleep(200 * 1000);
	obs_shutdown();
	printf("libobs allocations still alive after shutdown: %ld\n", bnum_allocs());
	printf("warnings/errors logged: %d, empty frames: %d\n", errors, failures);
	return failures ? 1 : 0;
}
