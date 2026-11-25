use super::*;

#[test]
fn only_a_command_or_an_edit_in_a_folder_says_where_it_runs() {
    assert!(runs_in_worktree(Some(kg_domain::ToolKind::Execute), true));
    assert!(runs_in_worktree(Some(kg_domain::ToolKind::Edit), true));
    assert!(!runs_in_worktree(Some(kg_domain::ToolKind::Read), true));
    assert!(!runs_in_worktree(Some(kg_domain::ToolKind::Execute), false));
}
