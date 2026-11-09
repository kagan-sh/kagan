use kg_domain::{Section, SectionId, Thread};

use super::{id, store, thread};
use crate::Store;

#[test]
fn sections_file_threads() {
    let store = store();
    let section = Section {
        id: SectionId("sec_1".into()),
        name: "Reviews".into(),
    };
    store.put_section(&section).unwrap();
    store
        .put_thread(&Thread {
            section_id: Some(section.id.clone()),
            ..thread("thr_1", 1)
        })
        .unwrap();
    assert_eq!(store.sections().unwrap(), std::slice::from_ref(&section));
    store.set_section(&id("thr_1"), None).unwrap();
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().section_id,
        None
    );
    store.set_section(&id("thr_1"), Some(&section.id)).unwrap();
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().section_id,
        Some(section.id.clone())
    );
    store.delete_section(&section.id).unwrap();
    assert!(store.sections().unwrap().is_empty());
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().section_id,
        None
    );
}
#[test]
fn reopening_a_file_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kagan.db");
    Store::open(&path)
        .unwrap()
        .put_section(&Section {
            id: SectionId("sec_1".into()),
            name: "Kept".into(),
        })
        .unwrap();
    assert_eq!(
        Store::open(&path).unwrap().sections().unwrap()[0].name,
        "Kept"
    );
}
