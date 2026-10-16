//! On macOS, embed `macos/Info.plist` in the program: as a launch agent it is
//! its own app to macOS, which needs the Camera and Microphone usage text
//! before it lets it ask (service.rs, `macos`).
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos/Info.plist");
        println!("cargo:rerun-if-changed={}", plist.display());
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
}
