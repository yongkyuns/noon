fn main() {
    // Manual macOS extension builds resolve CPython symbols from the interpreter.
    // Keep these link arguments scoped to this crate's cdylib target.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        pyo3_build_config::add_extension_module_link_args();
    }
}
