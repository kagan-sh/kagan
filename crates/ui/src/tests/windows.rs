use gpui_kit::{TestAppContext, WindowBounds};

#[gpui_kit::test]
fn the_about_window_is_a_fixed_titled_window(cx: &mut TestAppContext) {
    let options = cx.update(crate::about::options);
    assert!(matches!(
        options.window_bounds,
        Some(WindowBounds::Windowed(_))
    ));
    assert_eq!(
        options
            .titlebar
            .and_then(|titlebar| titlebar.title)
            .as_deref(),
        Some("About kagan")
    );
    assert!(!options.is_resizable);
    assert!(!options.is_minimizable);
}

#[gpui_kit::test]
fn the_start_failed_window_is_a_fixed_titled_window(cx: &mut TestAppContext) {
    let options = cx.update(crate::failed::options);
    assert!(matches!(
        options.window_bounds,
        Some(WindowBounds::Windowed(_))
    ));
    assert_eq!(
        options
            .titlebar
            .and_then(|titlebar| titlebar.title)
            .as_deref(),
        Some("kagan could not start")
    );
    assert!(!options.is_resizable);
    assert!(!options.is_minimizable);
}
