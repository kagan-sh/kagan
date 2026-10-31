fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = env("CARGO_MANIFEST_DIR");
    let icon = std::path::Path::new(&manifest).join("../../assets/brand/kagan.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let out = std::path::Path::new(&env("OUT_DIR")).join("kagan.rc");
    let path = icon.display().to_string().replace('\\', "\\\\");
    if let Err(error) = std::fs::write(&out, format!("1 ICON \"{path}\"\n")) {
        panic!("cannot write the kagan icon resource: {error}");
    }
    if let Err(error) = embed_resource::compile(&out, embed_resource::NONE).manifest_required() {
        panic!("cannot embed the kagan icon: {error:?}");
    }
}

fn env(key: &str) -> String {
    match std::env::var(key) {
        Ok(value) => value,
        Err(error) => panic!("{key} is set by cargo: {error}"),
    }
}
