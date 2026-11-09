use super::label;
#[test]
fn no_count_clears_the_badge() {
    assert_eq!(label(0), "");
}
