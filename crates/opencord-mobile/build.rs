// On Android, functions this library defines are bound inside it, so a
// library loaded beside it can't replace them (src/android_alloc.rs).
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-Bsymbolic-functions");
    }
}
