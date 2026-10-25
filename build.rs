#[path = "src/svg_logo.rs"]
mod svg_logo;
fn main() {
    println!("cargo:rerun-if-changed=assets/logo.svg");
    println!("cargo:rerun-if-changed=src/svg_logo.rs");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let svg =
        svg_logo::portable(&std::fs::read_to_string("assets/logo.svg").expect("Logo SVG source"))
            .expect("Portable SVG conversion");
    std::fs::write(out.join("logo.svg"), &svg).expect("Compiled SVG");
    let png = svg_logo::render(&svg, 256)
        .expect("SVG icon render")
        .encode_png()
        .expect("Icon PNG");
    let mut ico = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&png);
    std::fs::write(out.join("app.ico"), ico).expect("Windows icon");
    let resource = out.join("app.rc");
    std::fs::write(
        &resource,
        format!(
            "1 ICON \"{}\"",
            out.join("app.ico").display().to_string().replace('\\', "/")
        ),
    )
    .expect("Icon resource script");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile(resource, embed_resource::NONE)
            .manifest_required()
            .expect("Windows icon resource compilation failed");
    }
}
