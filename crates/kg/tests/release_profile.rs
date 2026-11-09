#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test stops at the first broken step"
)]

use std::path::Path;

#[test]
fn release_proc_macros_are_not_stripped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let toml = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let release = toml.split("[profile.release]").nth(1).unwrap();
    let (release, rest) = release
        .split_once("[profile.release.build-override]")
        .unwrap();
    assert!(release.contains("strip = \"symbols\""));
    let override_body = rest.split("\n[").next().unwrap();
    assert!(override_body.contains("strip = \"none\""));
}
