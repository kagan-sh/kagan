use super::chord;
use gpui_kit::{AppContext as _, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{InteractionOption, Op, OpResult, PluginId, PluginOp, PreferenceOp, ProjectId};

use kg_plugins::HostUi;

use crate::kagan::changes::POLL;
use kg_core::TYPING_QUIET;

use super::{
    PAGE, click, open, open_lines, open_surface, open_with_plugins, pending, repo_thread,
    run_hidden_shell, run_shell, set_preference, setup, shows, wait_until,
};

#[gpui_kit::test]
fn opening_again_brings_the_same_window_forward(cx: &mut TestAppContext) {
    let _data = setup(cx);
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn the_right_sidebar_opens_and_closes_from_its_shortcut(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    assert!(cx.update(|cx| kagan.read(cx).right_open()));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(!cx.update(|cx| kagan.read(cx).right_open()));
    assert!(!shows(cx, window, "needs-you"));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(cx.update(|cx| kagan.read(cx).right_open()));
    assert!(shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn a_question_waits_while_the_person_types(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, "x");
    cx.run_until_parked();

    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    };
    let _asked = cx.update(|cx| Workspace::ask(&id, "Ship the change?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship the change?"]
    });
    let question = pending(cx, &id).remove(0).id.0;
    assert!(shows(cx, window, "needs-held"));
    assert!(!shows(cx, window, &format!("answer-{question}-yes")));

    cx.executor().advance_clock(TYPING_QUIET);
    cx.run_until_parked();
    assert!(!shows(cx, window, "needs-held"));
    assert!(shows(cx, window, &format!("answer-{question}-yes")));
}

#[gpui_kit::test]
fn dock_count_tracks_distinct_threads_after_the_window_closes(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", second.0));
    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    };
    let _one = cx.update(|cx| Workspace::ask(&first, "First?".into(), vec![option.clone()], cx));
    let _two = cx.update(|cx| Workspace::ask(&first, "Second?".into(), vec![option.clone()], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            let attention = crate::kagan::attention::global(cx);
            let attention = attention.read(cx);
            attention.questions.len() == 2 && attention.count(cx) == 1
        })
    });
    cx.update(crate::about::show);
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let _three = cx.update(|cx| Workspace::ask(&second, "Third?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            let attention = crate::kagan::attention::global(cx);
            let attention = attention.read(cx);
            attention.questions.len() == 3 && attention.count(cx) == 2
        })
    });
}

#[gpui_kit::test]
fn questions_from_every_thread_wait_in_needs_you(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    };
    let _one = cx.update(|cx| Workspace::ask(&first, "Ship one?".into(), vec![option.clone()], cx));
    let _two = cx.update(|cx| Workspace::ask(&second, "Ship two?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)).len() == 2
    });
    let one = pending(cx, &first).remove(0).id.0;
    let two = pending(cx, &second).remove(0).id.0;
    assert!(shows(cx, window, &format!("answer-{one}-yes")));
    assert!(shows(cx, window, &format!("answer-{two}-yes")));
}

#[gpui_kit::test]
fn changes_lists_the_files_the_shown_thread_changed(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));

    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    wait_until(cx, |cx| shows(cx, window, "changes-not-git"));
    assert!(!shows(cx, window, "change-notes.txt"));
    click(cx, window, &format!("thread-{}", thread.0));
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
}

#[gpui_kit::test]
fn an_open_diff_follows_the_disk_without_a_click(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("two"))
    });

    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("three"))
    });
}

#[gpui_kit::test]
fn disabling_a_plugin_takes_its_panel_away(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, true);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    open_surface(cx, &PluginId(PAGE.into()), "page");
    wait_until(cx, |cx| shows(cx, window, "page-button"));

    let off = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Disable {
                id: PluginId(PAGE.into()),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(off).unwrap();
    wait_until(cx, |cx| !shows(cx, window, "page-button"));
}

#[gpui_kit::test]
fn the_right_sidebar_keeps_its_width_when_collapsed(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_right(px(420.), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    let dragged = cx.update(|cx| kagan.read(cx).right_width_now(cx));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(!cx.update(|cx| kagan.read(cx).right_open()));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    let reopened = cx.update(|cx| kagan.read(cx).right_width_now(cx));
    assert!(
        (reopened - dragged).abs() < 1.,
        "dragged {dragged}, reopened {reopened}"
    );
}

#[gpui_kit::test]
fn the_right_sidebar_open_state_is_saved(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    let saved = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: "main.right-sidebar".into(),
            }),
            cx,
        )
    });
    let OpResult::Setting(value) = cx.foreground_executor().block_test(saved).unwrap() else {
        panic!("setting");
    };
    assert_eq!(value, serde_json::json!(false));
}

#[gpui_kit::test]
fn a_failed_preference_read_is_not_an_unset_value(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    let unset = cx.update(|cx| crate::kagan::layout_read::<u8>("terminal-font-size", cx));
    assert_eq!(cx.foreground_executor().block_test(unset), Ok(None));
    set_preference(cx, "main.left-width", serde_json::json!(320.0));
    let wrong_shape = cx.update(|cx| crate::kagan::layout_read::<String>("main.left-width", cx));
    assert!(cx.foreground_executor().block_test(wrong_shape).is_err());
    let unknown = cx.update(|cx| crate::kagan::layout_read::<bool>("no-such-preference", cx));
    assert!(cx.foreground_executor().block_test(unknown).is_err());
}

#[gpui_kit::test]
fn a_question_on_a_hidden_thread_waits_in_needs_you(cx: &mut TestAppContext) {
    let (kagan, _window, _data) = open(cx);
    let id = run_hidden_shell(cx);
    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    };
    let _asked = cx.update(|cx| Workspace::ask(&id, "Hidden ask?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            let attention = crate::kagan::attention::global(cx);
            let attention = attention.read(cx);
            kagan.read(cx).question_titles(cx) == ["Hidden ask?"] && attention.count(cx) == 0
        })
    });
}

#[gpui_kit::test]
fn a_question_already_on_screen_stays_up_while_the_person_types(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let option = || InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    };
    let _first = cx.update(|cx| Workspace::ask(&id, "Ship one?".into(), vec![option()], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship one?"]
    });
    let first = pending(cx, &id).remove(0).id.0;
    assert!(shows(cx, window, &format!("answer-{first}-yes")));

    cx.simulate_keystrokes(window, "x");
    cx.run_until_parked();
    let _second = cx.update(|cx| Workspace::ask(&id, "Ship two?".into(), vec![option()], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)).len() == 2
    });
    let second = pending(cx, &id)
        .into_iter()
        .find(|asked| asked.title == "Ship two?")
        .expect("second question")
        .id
        .0;
    assert!(shows(cx, window, &format!("answer-{first}-yes")));
    assert!(!shows(cx, window, &format!("answer-{second}-yes")));
    assert!(shows(cx, window, "needs-held"));

    cx.executor().advance_clock(TYPING_QUIET);
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("answer-{second}-yes")));
}

#[gpui_kit::test]
fn a_plugin_panel_is_told_the_shown_thread_and_project(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, true);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.run_until_parked();
    let shown = cx.update(|cx| {
        HostUi::global(cx).and_then(|host| host.read(cx).window_thread(window.window_id()))
    });
    assert_eq!(shown, Some((Some(id), Some(ProjectId("prj_kagan".into())))));
}

#[gpui_kit::test]
fn tab_reaches_a_changed_file_and_enter_or_space_opens_it(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    let row = cx
        .update(|cx| kagan.read(cx).change_focus("notes.txt"))
        .expect("a focus handle for the row");

    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    for _ in 0..60 {
        if cx
            .update_window(window, |_, window, _| row.is_focused(window))
            .unwrap()
        {
            break;
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    assert!(
        cx.update_window(window, |_, window, _| row.is_focused(window))
            .unwrap(),
        "Tab did not reach the row"
    );

    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\n").unwrap();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("three")),
        "Enter did not open the file"
    );

    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.focus(&row, cx))
        .unwrap();
    cx.simulate_keystrokes(window, "space");
    cx.run_until_parked();
    assert!(
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("four")),
        "Space did not open the file"
    );
}

#[gpui_kit::test]
fn a_question_answered_elsewhere_leaves_needs_you(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let option = |id: &str| InteractionOption {
        id: id.into(),
        label: id.into(),
        role: None,
    };
    let _asked = cx.update(|cx| {
        Workspace::ask(
            &id,
            "Ship it?".into(),
            vec![option("yes"), option("no")],
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship it?"]
    });
    let question = pending(cx, &id).remove(0).id;
    assert!(shows(cx, window, &format!("answer-{}-yes", question.0)));

    let answered = cx.update(|cx| Workspace::answer(&id, question.clone(), "no".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx))
            .is_empty()
    });
    assert!(!shows(cx, window, &format!("answer-{}-yes", question.0)));
}
