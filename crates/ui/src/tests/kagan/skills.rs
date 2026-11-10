use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use crate::kagan::Kagan;
use crate::tests::kagan::{
    chord, click, focused_of, grid_text, label, open, shell, shows, wait_until,
};

fn show_skills(cx: &mut TestAppContext, kagan: &Entity<Kagan>, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.show_skills(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
}

fn write_user_skill(data: &std::path::Path, name: &str) {
    let folder = data.join("skills").join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A user skill.\n---\n\nDo it.\n"),
    )
    .unwrap();
}

#[gpui_kit::test]
fn the_skills_page_lists_the_built_in_skills(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    show_skills(cx, &kagan, window);
    assert!(shows(cx, window, "skill-row-kg-cli"));
}

#[gpui_kit::test]
fn the_skills_shortcut_opens_the_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("OpenSkills"));
    cx.run_until_parked();
    assert!(shows(cx, window, "skill-row-kg-cli"));
}

#[gpui_kit::test]
fn a_built_in_skill_opens_read_only(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    show_skills(cx, &kagan, window);
    click(cx, window, "skill-row-kg-cli");
    cx.run_until_parked();
    assert!(shows(cx, window, "skills-read-only"));
    assert!(!shows(cx, window, "skills-edit"));
    assert!(!shows(cx, window, "skills-delete"));
}

#[gpui_kit::test]
fn a_user_skill_can_be_deleted_after_confirming(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    click(cx, window, "skill-row-mine");
    cx.run_until_parked();
    click(cx, window, "skills-delete");
    assert!(shows(cx, window, "skills-confirm-delete"));
    assert!(data.path().join("skills/mine/SKILL.md").exists());
    click(cx, window, "skills-confirm-delete");
    cx.run_until_parked();
    assert!(!data.path().join("skills/mine").exists());
    assert!(!shows(cx, window, "skill-row-mine"));
}

#[gpui_kit::test]
fn a_user_skill_opens_an_editor(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    click(cx, window, "skill-row-mine");
    cx.run_until_parked();
    click(cx, window, "skills-edit");
    cx.run_until_parked();
    assert!(shows(cx, window, "skills-save"));
}

#[gpui_kit::test]
fn fork_opens_the_new_thread_screen_with_the_prompt_line(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    click(cx, window, "project-prj_kagan");
    cx.run_until_parked();
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    click(cx, window, "skill-row-mine");
    cx.run_until_parked();
    click(cx, window, "skills-fork");
    cx.run_until_parked();
    assert!(shows(cx, window, "start-fork-note"));
}

#[gpui_kit::test]
fn saving_an_edit_shows_the_new_description(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    click(cx, window, "skill-row-mine");
    cx.run_until_parked();
    click(cx, window, "skills-edit");
    cx.run_until_parked();
    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(
        window,
        "---\nname: mine\ndescription: Edited now.\n---\n\nNew body.\n",
    );
    cx.run_until_parked();
    click(cx, window, "skills-save");
    wait_until(cx, |cx| {
        label(cx, window, "skills-field-description") == "Edited now."
    });
    assert!(!shows(cx, window, "skills-save"));
}

#[gpui_kit::test]
fn a_new_skill_is_in_the_list_after_going_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    show_skills(cx, &kagan, window);
    click(cx, window, "skills-new");
    cx.run_until_parked();
    cx.simulate_input(window, "fresh-one");
    cx.run_until_parked();
    click(cx, window, "skills-create");
    wait_until(cx, |cx| shows(cx, window, "skills-back"));
    click(cx, window, "skills-back");
    cx.run_until_parked();
    assert!(shows(cx, window, "skill-row-fresh-one"));
}

#[gpui_kit::test]
fn a_skill_row_opens_from_the_keyboard_and_escape_goes_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    show_skills(cx, &kagan, window);
    for _ in 0..40 {
        if focused_of(cx, window, "skill-row-kg-cli") == Some(true) {
            break;
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    assert_eq!(focused_of(cx, window, "skill-row-kg-cli"), Some(true));
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(shows(cx, window, "skills-read-only"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(shows(cx, window, "skill-row-kg-cli"));
}

fn fork_to_start(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
    window: AnyWindowHandle,
    data: &std::path::Path,
) {
    click(cx, window, "project-prj_kagan");
    cx.run_until_parked();
    write_user_skill(data, "mine");
    show_skills(cx, kagan, window);
    click(cx, window, "skill-row-mine");
    cx.run_until_parked();
    click(cx, window, "skills-fork");
    cx.run_until_parked();
}

fn start_with_program(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
    window: AnyWindowHandle,
    program: &str,
) {
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| {
            picker.use_custom(program, "", window, cx);
        });
    })
    .unwrap();
    click(cx, window, "start");
}

#[gpui_kit::test]
fn a_forked_thread_gets_the_prompt_typed_in(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fork_to_start(cx, &kagan, window, data.path());
    start_with_program(cx, &kagan, window, shell());
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("Make a new skill"));
}

#[gpui_kit::test]
fn a_fork_types_nothing_when_its_thread_does_not_start(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fork_to_start(cx, &kagan, window, data.path());
    start_with_program(cx, &kagan, window, "/nonexistent/kagan-no-such-program");
    cx.run_until_parked();
    assert!(!grid_text(cx, &kagan).contains("Make a new skill"));
    assert!(!shows(cx, window, "plugin-prompt-notice"));
}

#[gpui_kit::test]
fn asking_for_the_open_page_again_keeps_its_place(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    show_skills(cx, &kagan, window);
    click(cx, window, "skill-row-kg-cli");
    cx.run_until_parked();
    show_skills(cx, &kagan, window);
    assert!(shows(cx, window, "skills-read-only"));
}

#[gpui_kit::test]
fn the_filter_menu_narrows_the_list_to_one_source(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    click(cx, window, "skills-filter");
    cx.run_until_parked();
    click(cx, window, "skills-filter-3");
    cx.run_until_parked();
    assert!(shows(cx, window, "skill-row-mine"));
    assert!(!shows(cx, window, "skill-row-kg-cli"));
}

fn first_row_by_tab(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<&'static str> {
    for _ in 0..80 {
        if focused_of(cx, window, "skill-row-mine") == Some(true) {
            return Some("mine");
        }
        if focused_of(cx, window, "skill-row-kg-cli") == Some(true) {
            return Some("kg-cli");
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    None
}

#[gpui_kit::test]
fn the_default_sort_lists_by_name(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    assert_eq!(first_row_by_tab(cx, window), Some("kg-cli"));
}

#[gpui_kit::test]
fn the_sort_menu_reverses_the_order(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "mine");
    show_skills(cx, &kagan, window);
    click(cx, window, "skills-sort");
    cx.run_until_parked();
    click(cx, window, "skills-sort-1");
    cx.run_until_parked();
    assert_eq!(first_row_by_tab(cx, window), Some("mine"));
}

#[gpui_kit::test]
fn a_new_skill_starts_from_the_template_picked_in_the_menu(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    show_skills(cx, &kagan, window);
    click(cx, window, "skills-new");
    cx.run_until_parked();
    cx.simulate_input(window, "review-me");
    cx.run_until_parked();
    click(cx, window, "skills-template");
    cx.run_until_parked();
    click(cx, window, "skills-template-ReviewPr");
    cx.run_until_parked();
    click(cx, window, "skills-create");
    wait_until(cx, |cx| shows(cx, window, "skills-back"));
    assert!(label(cx, window, "skills-field-description").starts_with("Review a pull request"));
}

fn first_row_among(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    names: &[&str],
) -> Option<String> {
    for _ in 0..80 {
        for name in names {
            if focused_of(cx, window, &format!("skill-row-{name}")) == Some(true) {
                return Some((*name).to_owned());
            }
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    None
}

#[gpui_kit::test]
fn project_first_keeps_names_in_order_inside_each_source(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    write_user_skill(data.path(), "beta-skill");
    write_user_skill(data.path(), "alpha-skill");
    show_skills(cx, &kagan, window);
    click(cx, window, "skills-sort");
    cx.run_until_parked();
    click(cx, window, "skills-sort-3");
    cx.run_until_parked();
    assert_eq!(
        first_row_among(cx, window, &["alpha-skill", "beta-skill"]).as_deref(),
        Some("alpha-skill")
    );
}
