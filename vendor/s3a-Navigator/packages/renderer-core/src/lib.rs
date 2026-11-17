// changed from upstream kitesurf 235111c6fe10f2d63f3476bf5e3d19b4c7089415
// Copyright (c) 2026 Cloudflare, Inc.
// Licensed under the Apache 2.0 license found in the LICENSE file or at:
// https://opensource.org/licenses/Apache-2.0

//! PageRenderer's trimmed Wasm entry point for scene replay, rasterization, and PDF output.

#[cfg(all(target_arch = "wasm32", not(feature = "native-host")))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg_attr(all(target_arch = "wasm32", not(feature = "native-host")), wasm_bindgen)]
#[cfg(all(target_arch = "wasm32", not(feature = "native-host")))]
extern "C" {
    #[cfg_attr(all(target_arch = "wasm32", not(feature = "native-host")), wasm_bindgen(js_namespace = console, js_name = warn))]
    fn console_warn(message: &str);
}

pub(crate) fn console_warn_str(message: &str) {
    #[cfg(all(target_arch = "wasm32", not(feature = "native-host")))]
    console_warn(message);
    #[cfg(not(all(target_arch = "wasm32", not(feature = "native-host"))))]
    eprintln!("{message}");
}

mod paint;
#[path = "../../core/src/ks_tracing.rs"]
mod ks_tracing;

#[cfg_attr(all(target_arch = "wasm32", not(feature = "native-host")), wasm_bindgen(start))]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
    ks_tracing::init();
}
