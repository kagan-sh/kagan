use super::{Fixture, op};
use gpui_kit::TestAppContext;
use kg_domain::*;

#[gpui_kit::test]
async fn the_guide_contains_its_map(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let OpResult::Text(guide) = op(Op::Guide { chapter: None }, cx).await.unwrap() else {
        panic!("guide")
    };
    assert!(guide.contains(include_str!("../guide-map.md")));
}
