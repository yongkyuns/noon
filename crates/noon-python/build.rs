fn main() {
    // Manual macOS extension builds resolve CPython symbols from the interpreter.
    // Keep these link arguments scoped to this crate's cdylib target.
    pyo3_build_config::add_extension_module_link_args();
}
