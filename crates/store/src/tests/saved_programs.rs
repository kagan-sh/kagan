use kg_domain::SavedProgram;

use super::store;
use crate::Error;

fn program(name: &str, line: &str, pinned: bool) -> SavedProgram {
    SavedProgram {
        name: name.into(),
        line: line.into(),
        pinned,
    }
}

#[test]
fn an_upsert_takes_the_new_line_and_keeps_the_pin() {
    let store = store();
    store
        .upsert_saved_program(&program("Careful", "claude --effort high", true))
        .unwrap();
    store
        .upsert_saved_program(&program("careful", "codex", false))
        .unwrap();
    let saved = store.saved_program("CAREFUL").unwrap().unwrap();
    assert_eq!(saved.line, "codex");
    assert!(saved.pinned);
    assert_eq!(store.saved_programs().unwrap().len(), 1);
}

#[test]
fn the_list_puts_pinned_names_first_then_sorts_names_ignoring_case() {
    let store = store();
    store
        .upsert_saved_program(&program("bravo", "codex", false))
        .unwrap();
    store
        .upsert_saved_program(&program("Alpha", "claude", false))
        .unwrap();
    store
        .upsert_saved_program(&program("Zulu", "amp", true))
        .unwrap();
    let names: Vec<_> = store
        .saved_programs()
        .unwrap()
        .into_iter()
        .map(|program| program.name)
        .collect();
    assert_eq!(names, ["Zulu", "Alpha", "bravo"]);
}

#[test]
fn rename_pin_and_delete_refuse_a_name_that_is_not_saved() {
    let store = store();
    store
        .upsert_saved_program(&program("Alpha", "claude", false))
        .unwrap();
    store.rename_saved_program("alpha", "Beta").unwrap();
    assert_eq!(store.saved_program("Beta").unwrap().unwrap().line, "claude");
    assert!(store.saved_program("Alpha").unwrap().is_none());
    store.set_saved_program_pinned("Beta", true).unwrap();
    assert!(store.saved_program("beta").unwrap().unwrap().pinned);
    store.delete_saved_program("Beta").unwrap();
    assert!(store.saved_programs().unwrap().is_empty());
    assert!(matches!(
        store.rename_saved_program("Beta", "Gamma"),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        store.set_saved_program_pinned("Beta", true),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        store.delete_saved_program("Beta"),
        Err(Error::NotFound { .. })
    ));
}
