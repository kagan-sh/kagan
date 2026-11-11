use kg_domain::{SavedPrompt, SavedPromptId};

use super::store;

fn prompt(id: &str, title: &str, body: &str, starred: bool) -> SavedPrompt {
    SavedPrompt {
        id: SavedPromptId(id.into()),
        title: title.into(),
        body: body.into(),
        starred,
    }
}

#[test]
fn search_matches_title_or_body_and_keeps_a_percent_sign_literal() {
    let store = store();
    store
        .insert_saved_prompt(&prompt("svp_plain", "Notes", "plain words", false))
        .unwrap();
    store
        .insert_saved_prompt(&prompt("svp_pct", "100%", "a percent", false))
        .unwrap();
    store
        .insert_saved_prompt(&prompt("svp_body", "Other", "Hello body", true))
        .unwrap();

    let titles = |text: &str| {
        store
            .search_saved_prompts(text)
            .unwrap()
            .into_iter()
            .map(|prompt| prompt.id.0)
            .collect::<Vec<_>>()
    };
    assert_eq!(titles("hello"), ["svp_body"]);
    assert_eq!(titles("NOTES"), ["svp_plain"]);
    assert_eq!(titles("%"), ["svp_pct"]);
    assert_eq!(titles(""), ["svp_body", "svp_pct", "svp_plain"]);
}

#[test]
fn starring_and_deleting_name_the_missing_row() {
    let store = store();
    let missing = SavedPromptId("svp_missing".into());
    let error = store.set_saved_prompt_starred(&missing, true).unwrap_err();
    assert!(matches!(error, crate::Error::NotFound { .. }));
    store
        .insert_saved_prompt(&prompt("svp_one", "One", "body", false))
        .unwrap();
    store
        .set_saved_prompt_starred(&SavedPromptId("svp_one".into()), true)
        .unwrap();
    assert!(
        store
            .saved_prompt(&SavedPromptId("svp_one".into()))
            .unwrap()
            .unwrap()
            .starred
    );
    store
        .delete_saved_prompt(&SavedPromptId("svp_one".into()))
        .unwrap();
    assert!(
        store
            .saved_prompt(&SavedPromptId("svp_one".into()))
            .unwrap()
            .is_none()
    );
    let error = store.delete_saved_prompt(&missing).unwrap_err();
    assert!(matches!(error, crate::Error::NotFound { .. }));
}
