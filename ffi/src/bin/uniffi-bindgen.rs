//! Generates the Kotlin/Swift sources for the library:
//! `cargo run -p ferrowl-ffi --features bindgen --bin uniffi-bindgen -- generate --library <lib> --language kotlin --out-dir <dir>`
fn main() {
    uniffi::uniffi_bindgen_main()
}
