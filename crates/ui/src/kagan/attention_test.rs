use kg_core::WorkspaceEvent;
use kg_domain::{ThreadId, WindowOp};

use super::refreshes;

#[test]
fn only_the_events_that_change_attention_refresh_it() {
    let id = ThreadId("thr_1".into());
    assert!(refreshes(&WorkspaceEvent::Changed));
    assert!(refreshes(&WorkspaceEvent::Waiting(id.clone())));
    assert!(!refreshes(&WorkspaceEvent::Finished(id)));
    assert!(!refreshes(&WorkspaceEvent::Window(WindowOp::Quit)));
}
