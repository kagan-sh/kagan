use agent_client_protocol::v1::schema::Diff;

use super::*;

#[test]
fn an_output_that_names_its_text_once_keeps_one_copy() {
    let content = vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
        TextContent::new("line one\nline two"),
    )))];
    let raw = serde_json::json!("line one\nline two");
    assert_eq!(
        output(&content, Some(&raw)),
        Some("line one\nline two".to_owned())
    );
    assert_eq!(
        output(&[], Some(&raw)),
        Some("line one\nline two".to_owned())
    );
}

#[test]
fn many_text_blocks_are_capped_together() {
    let content: Vec<ToolCallContent> = (0..4)
        .map(|_| {
            ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
                "é".repeat(TOOL_TEXT_MAX / 2),
            ))))
        })
        .collect();
    let joined = output(&content, None).expect("an output");
    assert!(
        joined.len() < TOOL_TEXT_MAX + CUT.len() + 4,
        "{}",
        joined.len()
    );
    assert!(joined.ends_with(CUT));
}

#[test]
fn an_input_too_large_to_carry_reads_as_a_marker() {
    let big = serde_json::json!({"file_path": "a.ts", "content": "x".repeat(TOOL_INPUT_MAX)});
    let kept = input(&big);
    let serde_json::Value::String(text) = &kept else {
        panic!("a marker: {kept:?}");
    };
    assert!(text.ends_with(CUT), "{}", &text[text.len() - 40..]);
    assert!(text.len() <= TOOL_INPUT_MAX + CUT.len());
    assert_eq!(
        input(&serde_json::json!({"path": "a.ts"})),
        serde_json::json!({"path": "a.ts"})
    );
}

#[test]
fn a_long_output_is_cut_at_a_character() {
    let long = "é".repeat(TOOL_TEXT_MAX);
    let cut = cap(long);
    assert!(cut.len() < TOOL_TEXT_MAX + CUT.len() + 4, "{}", cut.len());
    assert!(cut.ends_with(CUT), "{}", &cut[cut.len() - 40..]);
    assert_eq!(cap("short".into()), "short");
}

/// Both sides of a diff are capped, at a character boundary: a whole file's rewrite must not
/// travel in the log either.
#[test]
fn a_diff_too_long_is_cut_at_a_character_on_both_sides() {
    let long = "é".repeat(TOOL_TEXT_MAX);
    let content = vec![ToolCallContent::Diff(
        Diff::new("a.ts", long.clone()).old_text(long),
    )];
    let diff = diffs(&content).remove(0);
    let old = diff.old.expect("an old side");
    assert!(old.len() < TOOL_TEXT_MAX + CUT.len() + 4, "{}", old.len());
    assert!(old.ends_with(CUT), "{}", &old[old.len() - 40..]);
    assert!(diff.new.len() < TOOL_TEXT_MAX + CUT.len() + 4);
    assert!(
        diff.new.ends_with(CUT),
        "{}",
        &diff.new[diff.new.len() - 40..]
    );
}
