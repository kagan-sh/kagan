#[cfg(unix)]
use super::fake_program;
use super::{Fixture, op};
use gpui_kit::TestAppContext;
#[cfg(unix)]
use kg_domain::ProjectOp;
use kg_domain::{Op, OpError, OpResult, Program, ProgramOp, ProgramSource};

const CAREFUL: &str = "claude --model claude-opus-5-5 --effort high";

fn save(name: &str, line: &str) -> Op {
    Op::Program(ProgramOp::Save {
        name: name.into(),
        line: line.into(),
    })
}

async fn saved_list(cx: &mut TestAppContext) -> Vec<Program> {
    let OpResult::Programs(list) = op(Op::Program(ProgramOp::Discover { project_id: None }), cx)
        .await
        .unwrap()
    else {
        panic!("programs");
    };
    list.programs
        .into_iter()
        .filter(|program| program.source == ProgramSource::Saved)
        .collect()
}

#[gpui_kit::test]
async fn a_saved_program_is_listed_by_name_with_its_whole_line(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("Claude, careful", CAREFUL), cx).await.unwrap();
    let saved = saved_list(cx).await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].label, "Claude, careful");
    assert_eq!(saved[0].command, CAREFUL);
    assert!(!saved[0].pinned);
}

#[gpui_kit::test]
async fn saving_a_taken_name_takes_the_new_line_and_keeps_the_pin(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("Careful", CAREFUL), cx).await.unwrap();
    op(
        Op::Program(ProgramOp::Pin {
            name: "Careful".into(),
            pinned: true,
        }),
        cx,
    )
    .await
    .unwrap();
    op(save("careful", "codex --full-auto"), cx).await.unwrap();
    let saved = saved_list(cx).await;
    assert_eq!(saved.len(), 1, "a name is saved once, whatever its case");
    assert_eq!(saved[0].command, "codex --full-auto");
    assert!(saved[0].pinned);
}

#[gpui_kit::test]
async fn pinned_programs_are_listed_first_and_rename_keeps_the_line(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("Zulu", "codex"), cx).await.unwrap();
    op(save("Alpha", CAREFUL), cx).await.unwrap();
    op(
        Op::Program(ProgramOp::Pin {
            name: "Zulu".into(),
            pinned: true,
        }),
        cx,
    )
    .await
    .unwrap();
    let names: Vec<_> = saved_list(cx)
        .await
        .into_iter()
        .map(|program| program.label)
        .collect();
    assert_eq!(names, ["Zulu", "Alpha"]);

    op(
        Op::Program(ProgramOp::Rename {
            name: "Alpha".into(),
            to: "Claude, careful".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let renamed = saved_list(cx).await;
    assert_eq!(renamed[1].label, "Claude, careful");
    assert_eq!(renamed[1].command, CAREFUL);
}

#[gpui_kit::test]
async fn a_rename_onto_another_saved_name_is_refused(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("Alpha", CAREFUL), cx).await.unwrap();
    op(save("Beta", "codex"), cx).await.unwrap();
    let taken = op(
        Op::Program(ProgramOp::Rename {
            name: "Alpha".into(),
            to: "beta".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(taken, OpError::Invalid(_)), "{taken}");
    let names: Vec<_> = saved_list(cx)
        .await
        .into_iter()
        .map(|program| program.label)
        .collect();
    assert_eq!(names, ["Alpha", "Beta"], "a refused rename changes nothing");
}

#[gpui_kit::test]
async fn remove_deletes_the_name_and_a_missing_name_is_not_found(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("Alpha", CAREFUL), cx).await.unwrap();
    op(
        Op::Program(ProgramOp::Remove {
            name: "Alpha".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(saved_list(cx).await.is_empty());
    let missing = op(
        Op::Program(ProgramOp::Remove {
            name: "Alpha".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
}

#[gpui_kit::test]
async fn a_bad_name_or_line_is_refused_before_it_is_saved(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let blank = op(save("   ", CAREFUL), cx).await.unwrap_err();
    assert!(matches!(blank, OpError::Invalid(_)), "{blank}");
    let quote = op(save("Broken", "'claude"), cx).await.unwrap_err();
    assert!(matches!(quote, OpError::Invalid(_)), "{quote}");
    let long = "x".repeat(kg_domain::SAVED_NAME_MAX + 1);
    let too_long = op(save(&long, "codex"), cx).await.unwrap_err();
    assert!(matches!(too_long, OpError::Invalid(_)), "{too_long}");
    assert!(saved_list(cx).await.is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_project_can_default_to_a_saved_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let bin = fixture.dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    fake_program(&bin, "tool", "true");
    fake_program(&bin, "claude", "true");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.use_program_environment(&bin, std::path::Path::new("/bin/sh"));
    });
    op(save("Fast tool", "tool --fast"), cx).await.unwrap();
    op(
        Op::Project(ProjectOp::SetProgram {
            id: fixture.project.clone(),
            program: Some("tool --fast".into()),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Programs(list) = op(
        Op::Program(ProgramOp::Discover {
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("programs");
    };
    assert_eq!(list.default.as_deref(), Some("tool --fast"));
    let rows: Vec<_> = list
        .programs
        .iter()
        .filter(|program| program.command == "tool --fast")
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "a saved line is one row, not one more recent row"
    );
    assert_eq!(rows[0].label, "Fast tool");
    assert!(rows[0].installed());
}

#[gpui_kit::test]
async fn renaming_a_missing_name_is_not_found_even_when_the_new_name_is_taken(
    cx: &mut TestAppContext,
) {
    let _fixture = Fixture::new(cx).await;
    op(save("Beta", "codex"), cx).await.unwrap();
    let missing = op(
        Op::Program(ProgramOp::Rename {
            name: "Ghost".into(),
            to: "Beta".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
}

#[gpui_kit::test]
async fn a_saved_line_is_stored_trimmed_so_it_matches_a_recent_one(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("Padded", "  claude  "), cx).await.unwrap();
    let saved = saved_list(cx).await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].command, "claude");
}

#[gpui_kit::test]
async fn a_rename_onto_a_taken_name_says_so(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(save("alpha", "/bin/echo a"), cx).await.unwrap();
    op(save("beta", "/bin/echo b"), cx).await.unwrap();
    let error = op(
        Op::Program(ProgramOp::Rename {
            name: "alpha".into(),
            to: "beta".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "a saved program named beta already exists"
    );
}
