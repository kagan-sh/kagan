use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use kg_domain::OpError;

use super::{failure, run};

/// A stand-in for `screencapture`: `body` runs with the capture path as its last argument.
fn program(folder: &Path, body: &str) -> PathBuf {
    let script = folder.join("screencapture");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).expect("script is written");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("script is executable");
    script
}

fn staged_left_behind(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .expect("folder reads")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("kg-capture-")
        })
        .count()
}

#[test]
fn a_capture_that_writes_nothing_leaves_an_existing_image_alone() {
    let folder = tempfile::tempdir().expect("folder");
    let destination = folder.path().join("shot.png");
    std::fs::write(&destination, "OLD").expect("old image");
    let fake = program(folder.path(), "exit 0");

    assert_eq!(
        run(fake.as_os_str(), 1, &destination),
        Err(OpError::Failed(format!(
            "screencapture wrote no image to {}",
            destination.display()
        )))
    );
    assert_eq!(
        std::fs::read_to_string(&destination).expect("old image"),
        "OLD"
    );
    assert_eq!(staged_left_behind(folder.path()), 0);
}

#[test]
fn a_capture_that_writes_replaces_the_image() {
    let folder = tempfile::tempdir().expect("folder");
    let destination = folder.path().join("shot.png");
    std::fs::write(&destination, "OLD").expect("old image");
    // `last` is the capture path, which screencapture receives as its final argument.
    let fake = program(
        folder.path(),
        "for last; do :; done\nprintf NEW > \"$last\"",
    );

    assert_eq!(run(fake.as_os_str(), 1, &destination), Ok(()));
    assert_eq!(
        std::fs::read_to_string(&destination).expect("new image"),
        "NEW"
    );
    assert_eq!(staged_left_behind(folder.path()), 0);
}

#[test]
fn a_failed_capture_keeps_the_reason_macos_gave() {
    let output = Command::new("sh")
        .args([
            "-c",
            "printf 'could not create image from window\\n' >&2; exit 1",
        ])
        .output()
        .expect("sh runs");
    assert_eq!(
        failure(&output.stderr, output.status),
        "screencapture failed: could not create image from window"
    );
}
