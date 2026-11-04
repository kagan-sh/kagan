fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = std::env::var("CARGO_MANIFEST_DIR")?;
    let icon = std::path::Path::new(&manifest).join("../../assets/brand/kagan.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let out = std::path::Path::new(&std::env::var("OUT_DIR")?).join("kagan.rc");
    let path = icon.display().to_string().replace('\\', "\\\\");
    std::fs::write(&out, format!("1 ICON \"{path}\"\n"))
        .map_err(|error| format!("cannot write the kagan icon resource: {error}"))?;
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .map_err(|error| format!("cannot embed the kagan icon: {error}"))?;
    Ok(())
}
