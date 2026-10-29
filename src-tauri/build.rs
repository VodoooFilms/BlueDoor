fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-z,max-page-size=16384");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        use std::path::PathBuf;
        let architecture = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64",
            Ok("x86_64") => "x86_64",
            _ => panic!("unsupported macOS OCR target"),
        };
        for helper in ["bluedoor-ocr", "bluedoor-layout"] {
            let source = PathBuf::from(format!("ocr/{helper}.swift"));
            let destination = PathBuf::from(format!("resources/runtime/{helper}"));
            println!("cargo:rerun-if-changed={}", source.display());
            std::fs::create_dir_all(destination.parent().unwrap()).expect("OCR runtime directory");
            let status = std::process::Command::new("swiftc")
                .args(["-O", "-target", &format!("{architecture}-apple-macosx13.4")])
                .args([
                    "-framework",
                    "Vision",
                    "-framework",
                    "PDFKit",
                    "-framework",
                    "AppKit",
                ])
                .arg(&source)
                .arg("-o")
                .arg(&destination)
                .status()
                .expect("swiftc is required to build local OCR on macOS");
            assert!(status.success(), "could not compile {helper}");
        }
    }
    tauri_build::build()
}
