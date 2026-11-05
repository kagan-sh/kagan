#[test]
fn the_build_script_writes_the_windows_icon_resource() {
    let script = std::fs::read_to_string(std::path::Path::new(env!("OUT_DIR")).join("kagan.rc"))
        .expect("the build script wrote kagan.rc");
    assert!(script.starts_with("1 ICON \""), "{script}");
    assert!(script.trim_end().ends_with("kagan.ico\""), "{script}");
    assert_eq!(script.matches("ICON").count(), 1, "{script}");
}

#[test]
fn release_ms_does_not_enable_gpui_kit_test_support_on_kg_ui() {
    // A release build does not change feature resolution. The normal tree is the check.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("cargo")
        .current_dir(root)
        .args([
            "tree",
            "-e",
            "normal,features",
            "-p",
            "kg-ui",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo tree");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        assert!(
            !(line.contains("gpui-kit") && line.contains("test-support")),
            "kg-ui enables gpui-kit test-support in normal deps: {line}"
        );
    }
}
