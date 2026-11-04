use crate::page::{FILE_CAP, MESSAGE_CAP, PageFileError, parse_message, read_plugin_file};

#[test]
fn a_file_inside_the_plugin_is_read() {
    let root = tempfile::tempdir().unwrap();
    let page = root.path().join("ui");
    std::fs::create_dir(&page).unwrap();
    std::fs::write(page.join("index.html"), b"<p>hello</p>").unwrap();
    let bytes = read_plugin_file(root.path(), "ui/index.html").unwrap();
    assert_eq!(bytes, b"<p>hello</p>");
}

#[test]
fn a_path_that_leaves_the_plugin_is_refused() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("index.html"), b"inside").unwrap();
    for path in [
        "../index.html",
        "/etc/passwd",
        "ui/../../index.html",
        "..",
        r"ui\index.html",
    ] {
        assert_eq!(
            read_plugin_file(root.path(), path),
            Err(PageFileError::Refused),
            "{path}"
        );
    }
}

#[test]
fn a_missing_file_is_missing() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        read_plugin_file(root.path(), "ui/index.html"),
        Err(PageFileError::Missing)
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_plugin_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside.html");
    std::fs::write(&outside, b"secret-outside").unwrap();
    let plugin = root.path().join("plugin");
    std::fs::create_dir(&plugin).unwrap();
    std::os::unix::fs::symlink(&outside, plugin.join("index.html")).unwrap();
    let read = read_plugin_file(&plugin, "index.html");
    assert_eq!(read, Err(PageFileError::Refused));
    let shown = match &read {
        Ok(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Err(error) => error.to_string(),
    };
    assert!(!shown.contains("secret-outside"), "{shown}");
}

#[test]
fn a_page_file_error_is_an_error() {
    let error = PageFileError::TooBig;
    let reported: &dyn std::error::Error = &error;
    assert!(reported.to_string().contains("8 MiB"));
}

#[test]
fn a_file_over_the_cap_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("big.html");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(FILE_CAP + 1).unwrap();
    assert_eq!(
        read_plugin_file(root.path(), "big.html"),
        Err(PageFileError::TooBig)
    );
}

#[test]
fn a_page_message_is_json_within_the_cap() {
    assert_eq!(parse_message(r#"{"ok":true}"#).unwrap()["ok"], true);
    assert!(parse_message("not json").is_err());
    let huge = format!("\"{}\"", "x".repeat(MESSAGE_CAP));
    assert!(parse_message(&huge).unwrap_err().contains("over the"));
}

#[test]
fn the_reserved_folder_is_not_a_plugin_file() {
    let root = tempfile::tempdir().unwrap();
    let reserved = root.path().join("__kagan");
    std::fs::create_dir(&reserved).unwrap();
    std::fs::write(reserved.join("theme.css"), b"body{}").unwrap();
    assert_eq!(
        read_plugin_file(root.path(), "__kagan/theme.css"),
        Err(PageFileError::Refused)
    );
    assert_eq!(
        read_plugin_file(root.path(), "__KAGAN/theme.css"),
        Err(PageFileError::Refused)
    );
}
