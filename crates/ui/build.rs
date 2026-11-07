use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    // One name, so a cfg of these three systems stays on one line.
    println!("cargo:rustc-check-cfg=cfg(desktop)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(os.as_str(), "linux" | "macos" | "windows") {
        println!("cargo:rustc-cfg=desktop");
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest.join("../..");
    // A source tarball unpacked inside another repository must not take that repository's commit.
    let ours = git(&root, &["rev-parse", "--show-toplevel"])
        .is_some_and(|top| same(Path::new(&top), &root));
    if !ours {
        return Ok(());
    }
    // The HEAD log moves on every commit, checkout and pull, even when the branch ref is packed.
    for name in ["HEAD", "logs/HEAD"] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", name]) {
            let path = root.join(path);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    if let Some(commit) = git(&root, &["rev-parse", "--short=7", "HEAD"]) {
        println!("cargo:rustc-env=KG_COMMIT={commit}");
    }
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_owned())
}

fn same(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}
