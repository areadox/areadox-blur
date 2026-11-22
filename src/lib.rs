//! Areadox Blur – a Composite-Blur-style blur filter for OBS Studio 33+,
//! written in Rust.
//!
//! This file contains the libobs module entry points (the equivalent of
//! `OBS_DECLARE_MODULE()` and `OBS_MODULE_USE_DEFAULT_LOCALE()` in C).

#![allow(clippy::missing_safety_doc)] // the unsafe fns are libobs entry points

mod ffi;
mod filter;
mod obs;
mod props;
mod render;

use ffi::*;
use std::ffi::{c_char, CStr};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

static MODULE: AtomicPtr<obs_module_t> = AtomicPtr::new(ptr::null_mut());
static LOOKUP: AtomicPtr<lookup_t> = AtomicPtr::new(ptr::null_mut());

pub(crate) fn current_module() -> *mut obs_module_t {
    MODULE.load(Ordering::Acquire)
}

// --- OBS_DECLARE_MODULE() -------------------------------------------------

#[no_mangle]
pub extern "C" fn obs_module_set_pointer(module: *mut obs_module_t) {
    MODULE.store(module, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn obs_module_ver() -> u32 {
    LIBOBS_API_VER
}

// --- OBS_MODULE_USE_DEFAULT_LOCALE("areadox-blur", "en-US") ---------------

#[no_mangle]
pub unsafe extern "C" fn obs_module_set_locale(locale: *const c_char) {
    let old = LOOKUP.swap(ptr::null_mut(), Ordering::AcqRel);
    if !old.is_null() {
        text_lookup_destroy(old);
    }
    let lookup = obs_module_load_locale(current_module(), c"en-US".as_ptr(), locale);
    LOOKUP.store(lookup, Ordering::Release);
}

#[no_mangle]
pub unsafe extern "C" fn obs_module_free_locale() {
    let old = LOOKUP.swap(ptr::null_mut(), Ordering::AcqRel);
    if !old.is_null() {
        text_lookup_destroy(old);
    }
}

#[no_mangle]
pub unsafe extern "C" fn obs_module_get_string(
    lookup_string: *const c_char,
    out: *mut *const c_char,
) -> bool {
    let lookup = LOOKUP.load(Ordering::Acquire);
    !lookup.is_null() && text_lookup_getstr(lookup, lookup_string, out)
}

/// Translate a locale key. Falls back to the key itself.
pub(crate) fn text(key: &CStr) -> *const c_char {
    let mut out: *const c_char = key.as_ptr();
    unsafe { obs_module_get_string(key.as_ptr(), &mut out) };
    out
}

// --- metadata ---------------------------------------------------------------

#[no_mangle]
pub extern "C" fn obs_module_name() -> *const c_char {
    c"Areadox Blur".as_ptr()
}

#[no_mangle]
pub extern "C" fn obs_module_description() -> *const c_char {
    text(c"Plugin.Description")
}

#[no_mangle]
pub extern "C" fn obs_module_author() -> *const c_char {
    c"areadox".as_ptr()
}

// --- load / unload ----------------------------------------------------------

#[no_mangle]
pub extern "C" fn obs_module_load() -> bool {
    obs::guard(false, || {
        filter::register();
        obs::info!(
            "version {} loaded (libobs API {}.{})",
            env!("CARGO_PKG_VERSION"),
            LIBOBS_API_MAJOR_VER,
            LIBOBS_API_MINOR_VER
        );
        true
    })
}

#[no_mangle]
pub extern "C" fn obs_module_unload() {
    obs::info!("unloaded");
}
