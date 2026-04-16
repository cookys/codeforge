fn main() {
    // Re-run this build script (and recompile the crate) whenever locale YAML files change.
    // Without this, `cargo build` caches the binary even after locales/*.yaml is edited,
    // because Cargo doesn't know the i18n!() macro embeds those files at compile time.
    println!("cargo:rerun-if-changed=locales/");
}
