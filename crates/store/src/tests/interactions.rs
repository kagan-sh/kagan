use kg_domain::{InteractionOption, PendingInteraction, PendingInteractionId, Timestamp};

use super::{id, store, thread};

#[test]
fn pending_interactions() {
    let store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    let interaction = PendingInteraction {
        id: PendingInteractionId("pi_1".into()),
        thread_id: id("thr_1"),
        title: "Allow edit of src/lib.rs?".into(),
        options: vec![
            InteractionOption {
                id: "allow".into(),
                label: "Allow".into(),
            },
            InteractionOption {
                id: "reject".into(),
                label: "Reject".into(),
            },
        ],
        fields: None,
        created_at: Timestamp(3),
    };
    store.put_pending_interaction(&interaction).unwrap();
    assert_eq!(
        store.pending_interactions(&id("thr_1")).unwrap(),
        std::slice::from_ref(&interaction)
    );
    store.delete_pending_interaction(&interaction.id).unwrap();
    assert!(store.pending_interactions(&id("thr_1")).unwrap().is_empty());
}
