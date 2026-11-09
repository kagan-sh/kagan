use super::{Cache, MAX_ENTRIES, cached};

#[test]
fn the_cache_holds_a_bounded_number_of_scopes() {
    let cache = Cache::new();
    for index in 0..200 {
        cached(&cache, &format!("scope-{index}"), "left", "right", |_| ());
    }
    assert!(cache.len() <= MAX_ENTRIES, "{} scopes kept", cache.len());
}
