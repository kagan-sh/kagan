#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

#[test]
fn third_party_notices_match_the_lock_and_the_packager() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let notices = std::fs::read_to_string(root.join("assets/THIRD-PARTY-NOTICES.txt")).unwrap();
    let packager = std::fs::read_to_string(manifest.join("Cargo.toml")).unwrap();
    let lock = lock_hash(&root.join("Cargo.lock"));

    assert!(
        notices.contains(&format!("cargo-lock {lock}")),
        "regenerate with scripts/third-party-notices.sh"
    );
    for needle in [
        "SIL Open Font License",
        "Inter",
        "JetBrains Mono",
        "Copyright Amazon.com, Inc. or its affiliates.",
        "Apache License",
        "Permission is hereby granted",
        "THE SOFTWARE IS PROVIDED",
    ] {
        assert!(notices.contains(needle), "{needle}");
    }
    assert!(!notices.contains("&quot;"));
    assert!(notices.len() > 100_000);
    assert!(packager.contains("assets/THIRD-PARTY-NOTICES.txt"));
}

fn lock_hash(path: &Path) -> String {
    use sha2::{Digest as _, Sha256};
    hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}
