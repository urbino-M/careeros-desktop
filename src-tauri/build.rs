fn main() {
    println!("cargo:rerun-if-changed=resources/helpers/cv-extract.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("cv-extract");
        let arch = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") { "arm64" } else { "x86_64" };
        let status = std::process::Command::new("xcrun").args([
            "swiftc", "-O", "-target", &format!("{arch}-apple-macosx12.0"),
            "resources/helpers/cv-extract.swift", "-o",
        ]).arg(&output).arg("-module-cache-path").arg(output.parent().expect("parent").join("swift-cache")).status().expect("Swift compiler required for bundled local PDF OCR");
        assert!(status.success(), "Failed to compile local PDF OCR helper");
    }
    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new()
                .app_manifest(include_str!("windows-app-manifest.xml")),
        ),
    )
    .expect("failed to run tauri-build");
}
