fn main() {
    // The Common Controls 6 manifest (which the dialogs need) is embedded through the
    // linker rather than by tauri-build's resource, because tauri-build's resource only
    // reaches binaries: the unit tests, which run a mock Tauri app, would otherwise fail
    // to start with STATUS_ENTRYPOINT_NOT_FOUND. The manifest is the one tauri-build ships.
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("tauri-build");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        // A malformed manifest is a link error, not a silently missing manifest.
        println!("cargo:rustc-link-arg=/WX");
    }
}
