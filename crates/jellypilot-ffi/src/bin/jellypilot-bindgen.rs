//! Binding generator for `jellypilot-ffi`.
//!
//! Thin wrapper over UniFFI's own CLI so the dispatcher can run
//! `jellypilot-bindgen generate --library <libjellypilot_ffi.so> --language
//! kotlin --out-dir <dir>` with conventional uniffi-bindgen arguments.

fn main() {
    uniffi::uniffi_bindgen_main();
}
