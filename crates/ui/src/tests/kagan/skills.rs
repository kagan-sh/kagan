use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use crate::kagan::Kagan;
use crate::tests::kagan::{click, open, shows};

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
