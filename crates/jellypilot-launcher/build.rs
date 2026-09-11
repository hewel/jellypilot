//! Embeds the application icon into the Windows executable so Explorer,
//! shortcuts, and the taskbar show it. No-op on other targets.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    // rc.exe resolves quoted paths with escaped backslashes.
    let icon = icon.display().to_string().replace('\\', "\\\\");
    let rc = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("jellypilot.rc");
    std::fs::write(&rc, format!("1 ICON \"{icon}\"\n")).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed Windows icon resource");
}
