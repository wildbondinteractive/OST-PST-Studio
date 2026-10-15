// Windows version resource for ostpst.exe, so the file carries a product name, version,
// description and icon like the desktop application does. The version comes from the
// workspace version in Cargo.toml. No effect on other targets.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    #[cfg(windows)]
    {
        let mut res = tauri_winres::WindowsResource::new();
        res.set("ProductName", "OST-PST Studio")
            .set("FileDescription", "OST-PST Studio command line (ostpst)")
            .set("CompanyName", "OST/PST Studio contributors")
            .set(
                "LegalCopyright",
                "MIT License. Engine based on pstfree (MIT).",
            )
            .set("OriginalFilename", "ostpst.exe")
            .set("InternalName", "ostpst")
            .set_icon("../../src-tauri/icons/icon.ico");
        res.compile()
            .expect("compile the Windows version resource for ostpst.exe");
    }
}
