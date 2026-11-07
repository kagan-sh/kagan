use super::*;

#[test]
fn a_missing_runtime_names_the_download() {
    let message = failure(&format!(
        "creating the environment failed: {}",
        hresult("80070002")
    ));
    assert!(message.contains("WebView2 Runtime"));
    assert!(message.contains("https://developer.microsoft.com/microsoft-edge/webview2/"));
    let other = failure("the window is gone");
    assert!(other.contains("could not open"));
    assert!(!other.contains("WebView2 Runtime"));
    let x11 = failure("WebView on Linux requires an X11 window; start the application");
    assert!(x11.contains("DISPLAY"));
    assert!(!x11.contains("could not open"));
}

#[test]
fn webview_files_stay_inside_kagan_data() {
    let data = tempfile::tempdir().expect("temp data");
    let stale = data.path().join("page-stale");
    std::fs::create_dir_all(&stale).expect("stale page dir");
    std::fs::write(stale.join("lock"), b"x").expect("stale file");
    SWEPT.store(false, std::sync::atomic::Ordering::SeqCst);
    let path = page_dir(data.path()).expect("page dir");
    assert!(path.starts_with(data.path()));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    assert!(name.starts_with("page-"));
    assert!(
        !stale.exists(),
        "abandoned page dirs are removed on restart"
    );
    let _ = std::fs::remove_dir_all(path);
}
