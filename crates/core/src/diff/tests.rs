use super::{Cache, MAX_ENTRIES, cached};

#[test]
fn the_cache_holds_a_bounded_number_of_scopes() {
    let cache = Cache::new();
    for index in 0..200 {
        cached(&cache, &format!("scope-{index}"), "left", "right", |_| ());
    }
    assert!(cache.len() <= MAX_ENTRIES, "{} scopes kept", cache.len());
}

#[test]
fn a_cache_keeps_its_scopes_until_it_is_full() {
    let cache = Cache::new();
    for index in 0..3 {
        cached(&cache, &format!("scope-{index}"), "left", "right", |_| ());
    }
    assert_eq!(cache.len(), 3);
    for index in 3..MAX_ENTRIES {
        cached(&cache, &format!("scope-{index}"), "left", "right", |_| ());
    }
    cached(&cache, "scope-0", "left", "right", |_| ());
    assert_eq!(cache.len(), MAX_ENTRIES);
    cached(&cache, "scope-new", "left", "right", |_| ());
    assert_eq!(cache.len(), 1);
}
