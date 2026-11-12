use super::{HOST, URL, allowed};

#[test]
fn the_endpoint_is_the_pinned_chatgpt_address() {
    let parsed = reqwest::Url::parse(URL).unwrap();
    assert_eq!(parsed.scheme(), "https");
    assert_eq!(parsed.host_str(), Some(HOST));
    assert_eq!(parsed.path(), "/backend-api/wham/usage");
    assert!(allowed(URL));
}

#[test]
fn the_token_never_goes_to_another_host() {
    for url in [
        "https://evil.example/x",
        "https://chatgpt.com.evil.example/x",
        "https://chatgpt.com@evil.example/x",
        "https://evil.example@chatgpt.com.evil.example/",
        "http://chatgpt.com/backend-api/wham/usage",
        "ftp://chatgpt.com/backend-api/wham/usage",
        "not a url",
    ] {
        assert!(!allowed(url), "{url} must not get the token");
    }
}

#[test]
fn the_live_source_reads_the_pinned_endpoint() {
    assert_eq!(super::Source::live().url, URL);
}
