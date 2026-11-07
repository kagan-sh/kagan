use super::{is_web, page_url, secure};

#[test]
fn a_bare_name_opens_over_https() {
    let url = page_url(" example.com ").map(|url| url.to_string());
    assert_eq!(url.as_deref(), Some("https://example.com/"));
}

#[test]
fn a_local_host_opens_over_http() {
    let url = |typed| page_url(typed).map(|url| url.to_string());
    assert_eq!(
        url("localhost:3000").as_deref(),
        Some("http://localhost:3000/")
    );
    assert_eq!(
        url("127.0.0.1:5173/a").as_deref(),
        Some("http://127.0.0.1:5173/a")
    );
    assert_eq!(
        url("localhost.example.com").as_deref(),
        Some("https://localhost.example.com/")
    );
}

#[test]
fn only_web_addresses_open() {
    assert!(page_url("http://example.com/a").is_some());
    assert!(page_url("file:///etc/passwd").is_none());
    assert!(page_url("javascript:alert(1)").is_none());
    assert!(page_url("   ").is_none());
}

#[test]
fn an_https_address_is_marked_secure() {
    assert!(secure("https://example.com"));
    assert!(secure(" example.com "));
    assert!(!secure("http://example.com"));
    assert!(!secure("javascript:alert(1)"));
}

#[test]
fn only_web_pages_may_be_navigated_to() {
    assert!(is_web("https://example.com/next"));
    assert!(!is_web("file:///etc/passwd"));
    assert!(!is_web("about:blank"));
}
