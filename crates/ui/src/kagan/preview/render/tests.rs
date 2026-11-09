use super::*;

// The harness cannot see a TextView's image source, so this proves the
// resolver's payload; the `.image_source(document_image)` wiring is read, not run.
#[test]
fn a_markdown_image_resolves_to_nothing() {
    let uri = gpui_kit::SharedUri::from("https://example.com/pixel.png");
    let ImageSource::Image(image) = document_image(&uri) else {
        panic!("a document image must not become a resource")
    };
    assert!(image.bytes.is_empty());
}

// Likewise, the harness cannot observe `cx.open_url`; this proves the
// production decision the handler calls, not that the handler is wired.
#[test]
fn a_link_opens_only_on_the_web() {
    assert_eq!(web_link("https://example.com"), Some("https://example.com"));
    assert_eq!(
        web_link("http://localhost:3000"),
        Some("http://localhost:3000")
    );
    for denied in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:text/html,x",
    ] {
        assert_eq!(web_link(denied), None, "{denied} must not open");
    }
}
