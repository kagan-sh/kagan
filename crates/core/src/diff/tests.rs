use super::{Cache, cached};

#[test]
fn the_cache_holds_a_bounded_number_of_scopes() {
    const LIMIT: usize = 64;
    let cache = Cache::new();
    for index in 0..200 {
        cached(&cache, &format!("scope-{index}"), "left", "right", |_| ());
    }
    assert!(cache.len() <= LIMIT, "{} scopes kept", cache.len());
}
