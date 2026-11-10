use std::time::Duration;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Pixels, Point, TestAppContext, point, px,
};
use kg_domain::{Color, Op, OpResult, PluginId, PluginOp, ThemeFile, ThemeOp};
use serde_json::json;

use super::{
    THEMES, TIDE, aria_label, get_theme, install_theme_plugin, painted_bg, run_op, set_theme,
    try_op,
};
use crate::tests::kagan::{
    click, hover_to, open, open_with_plugins_then, present, selected_of, shows, value_of,
    wait_until,
};
use crate::theme::ThemeState;

#[gpui_kit::test]
fn appearance_shows_a_missing_theme_and_keeps_its_id(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_theme(cx, "ocean");
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    assert_eq!(
        aria_label(cx, window, "theme-error").as_deref(),
        Some("Theme ocean is missing. Using Kagan until it returns.")
    );
    assert_eq!(get_theme(cx).theme, "ocean");
    assert_eq!(selected_of(cx, window, "theme-fjord"), Some(false));
}

#[gpui_kit::test]
fn a_broken_theme_file_shows_its_error_on_its_card(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tide.json"), "{").unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    let label = aria_label(cx, window, "theme-tide").unwrap_or_default();
    assert!(label.starts_with("tide theme, does not load: "), "{label}");
    assert_eq!(selected_of(cx, window, "theme-fjord"), Some(true));

    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        aria_label(cx, window, "theme-tide").as_deref(),
        Some("Tide theme")
    );
    click(cx, window, "theme-tide");
    assert_eq!(selected_of(cx, window, "theme-tide"), Some(true));
}

#[gpui_kit::test]
fn an_oversized_odd_or_special_theme_file_shows_on_its_card(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("big.json"),
        format!("{TIDE}{}", " ".repeat(64 * 1024)),
    )
    .unwrap();
    let long = TIDE.replace("\"Tide\"", &format!("\"{}\"", "t".repeat(65)));
    std::fs::write(path.join("long.json"), long).unwrap();
    #[cfg(unix)]
    assert!(
        std::process::Command::new("mkfifo")
            .arg(path.join("pipe.json"))
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    let mut cards = vec![
        ("theme-big", "big theme, does not load: larger than 64 KiB"),
        (
            "theme-long",
            "long theme, does not load: theme name must be one line of at most 64 characters",
        ),
    ];
    if cfg!(unix) {
        cards.push((
            "theme-pipe",
            "pipe theme, does not load: not a regular file",
        ));
    }
    for (card, label) in cards {
        assert_eq!(aria_label(cx, window, card).as_deref(), Some(label));
    }
    assert_eq!(
        aria_label(cx, window, "theme-tide").as_deref(),
        Some("Tide theme"),
        "the folder still loads"
    );
}

#[gpui_kit::test]
fn an_invalid_edit_keeps_the_last_good_theme_and_says_so_once(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    set_theme(cx, "tide");
    let tide = painted_bg(cx);
    assert_eq!(tide, Color::new(0xf2_f2f2));

    std::fs::write(path.join("tide.json"), TIDE.replace("#f2f2f2", "nope")).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(painted_bg(cx), tide, "the last good colors stay");
    let message = aria_label(cx, window, "toast-message").unwrap_or_default();
    assert!(
        message.starts_with("Theme tide has an error, so the window keeps Tide: "),
        "{message}"
    );
    click(cx, window, "toast-dismiss");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(!shows(cx, window, "toast"), "the error is said once");

    std::fs::write(path.join("tide.json"), TIDE.replace("#f2f2f2", "#fafafa")).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        painted_bg(cx),
        Color::new(0xfa_fafa),
        "a fixed file applies"
    );
}

#[gpui_kit::test]
fn appearance_lists_built_in_user_and_plugin_themes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_theme_plugin);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    for (card, label) in [
        ("theme-fjord", "Ember Fjord theme, in use"),
        ("theme-tide", "Tide theme"),
        ("theme-dev.test.themes/midnight", "Midnight theme"),
    ] {
        assert_eq!(aria_label(cx, window, card).as_deref(), Some(label));
    }

    click(cx, window, "theme-dev.test.themes/midnight");
    assert_eq!(get_theme(cx).theme, "dev.test.themes/midnight");
    assert_eq!(painted_bg(cx), Color::new(0x10_1010));
}

#[gpui_kit::test]
fn disabling_a_plugin_drops_its_theme_and_says_so_once(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_theme_plugin);
    set_theme(cx, "dev.test.themes/midnight");
    assert_eq!(painted_bg(cx), Color::new(0x10_1010));

    run_op(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(THEMES.into()),
        }),
    );
    click(cx, window, "settings-gear");
    assert!(!present(cx, window, "theme-dev.test.themes/midnight"));
    assert_eq!(
        aria_label(cx, window, "toast-message").as_deref(),
        Some("Theme dev.test.themes/midnight went away, so the window uses Ember Fjord.")
    );
    assert_eq!(painted_bg(cx), Color::new(0xee_f1f5), "the default paints");
    assert_eq!(get_theme(cx).theme, "dev.test.themes/midnight");

    run_op(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(THEMES.into()),
        }),
    );
    assert_eq!(
        painted_bg(cx),
        Color::new(0x10_1010),
        "it returns with the plugin"
    );
}

#[gpui_kit::test]
fn new_theme_copies_the_theme_in_use_and_opens_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    click(cx, window, "theme-new");
    let path = data.path().join("themes").join("fjord-copy.json");
    let file = ThemeFile::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file.name, "Ember Fjord copy");
    let url = cx.opened_url().unwrap_or_default();
    assert!(
        url.starts_with("file://") && url.ends_with("/themes/fjord-copy.json"),
        "{url}"
    );

    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        aria_label(cx, window, "theme-fjord-copy").as_deref(),
        Some("Ember Fjord copy theme")
    );
    click(cx, window, "theme-new");
    assert!(data.path().join("themes/fjord-copy-2.json").is_file());
}

#[gpui_kit::test]
fn kg_theme_new_copies_a_named_theme_and_keeps_existing_files(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let new = |id: &str, from: Option<&str>| {
        Op::Theme(ThemeOp::New {
            id: id.into(),
            from: from.map(Into::into),
        })
    };
    let made = try_op(cx, new("warm", Some("ember")));
    let path = dunce::canonicalize(data.path())
        .unwrap()
        .join("themes/warm.json");
    assert_eq!(made.unwrap(), OpResult::Files(vec![path.clone()]));
    let file = ThemeFile::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file.name, "Ember copy");

    let again = try_op(cx, new("warm", Some("grey")))
        .unwrap_err()
        .to_string();
    assert!(again.ends_with("warm.json already exists"), "{again}");
    let built_in = try_op(cx, new("ember", None)).unwrap_err().to_string();
    assert_eq!(built_in, "theme ember is built in; choose another id");
    let missing = try_op(cx, new("cold", Some("ocean")))
        .unwrap_err()
        .to_string();
    assert_eq!(missing, "theme ocean");
}

#[gpui_kit::test]
fn open_themes_folder_makes_the_folder_and_opens_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    assert!(!path.exists());
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    click(cx, window, "theme-folder");
    assert!(path.is_dir());
    let url = cx.opened_url().unwrap_or_default();
    assert!(
        url.starts_with("file://") && url.ends_with("/themes"),
        "{url}"
    );
}

#[gpui_kit::test]
fn density_scales_the_look_and_is_saved(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let default = cx.update(|cx| crate::theme::look(cx).space.xs);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    click(cx, window, "density-roomy");
    wait_until(cx, |cx| {
        cx.update(|cx| crate::theme::look(cx).space.xs) != default
    });
    assert_eq!(
        cx.update(|cx| crate::theme::look(cx).space.xs),
        default * 125 / 100
    );
    let roomy = cx.update(|cx| crate::theme::look(cx));
    assert_eq!(
        (roomy.row, roomy.control),
        (35, 30),
        "row and control do not scale"
    );
    assert_eq!(
        try_op(
            cx,
            Op::Preference(kg_domain::PreferenceOp::Get {
                key: "density".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!("roomy"))
    );
    click(cx, window, "density-compact");
    wait_until(cx, |cx| {
        cx.update(|cx| crate::theme::look(cx).space.xs) != default * 125 / 100
    });
    assert_eq!(
        cx.update(|cx| crate::theme::look(cx).space.xs),
        default * 75 / 100
    );
}

#[gpui_kit::test]
fn a_previewed_theme_is_put_back_when_it_ends(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    cx.run_until_parked();
    let saved = painted_bg(cx);
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, |state, cx| state.preview("ember", cx));
    });
    cx.run_until_parked();
    assert_ne!(painted_bg(cx), saved, "the preview did not show");
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, ThemeState::end_preview);
    });
    cx.run_until_parked();
    assert_eq!(painted_bg(cx), saved);
}

#[gpui_kit::test]
fn keyboard_focus_on_a_card_previews_and_leaving_it_restores(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    cx.run_until_parked();
    let saved = painted_bg(cx);
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, |state, cx| state.follow_focus(Some("ember"), cx));
    });
    cx.run_until_parked();
    assert_ne!(painted_bg(cx), saved, "focus did not preview");
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, |state, cx| state.follow_focus(None, cx));
    });
    cx.run_until_parked();
    assert_eq!(painted_bg(cx), saved);
}

#[gpui_kit::test]
fn a_saved_font_is_used_when_installed_and_the_default_otherwise(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    assert!(present(cx, window, "setting-row-terminal-font"));
    let set = |cx: &mut TestAppContext, name: &str| {
        let task = cx.update(|cx| {
            kg_core::Workspace::dispatch(
                Op::Preference(kg_domain::PreferenceOp::Set {
                    key: "font-terminal".into(),
                    value: json!(name),
                }),
                cx,
            )
        });
        cx.foreground_executor().block_test(task).unwrap();
        cx.run_until_parked();
    };
    let mono = |cx: &mut TestAppContext| cx.update(|cx| cx.theme().mono_font_family.to_string());
    set(cx, "No Such Family Anywhere");
    assert_eq!(mono(cx), "JetBrains Mono", "a missing font falls back");
    let installed = cx.update(|cx| {
        crate::theme::ThemeState::global(cx)
            .read(cx)
            .installed_fonts
            .clone()
    });
    if let Some(name) = installed.first() {
        set(cx, name);
        assert_eq!(&mono(cx), name, "an installed font is used");
    }
}

fn center_of(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> Point<Pixels> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into()))
            .map(|element| element.bounds.center())
    })
    .unwrap()
    .unwrap_or_else(|| panic!("{id} is not on screen"))
}

fn preview_ember(cx: &mut TestAppContext) {
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, |state, cx| state.preview("ember", cx));
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_preview_survives_the_store_refresh_tick(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    cx.run_until_parked();
    let saved = painted_bg(cx);
    preview_ember(cx);
    let shown = painted_bg(cx);
    assert_ne!(shown, saved, "the preview did not show");
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(
        painted_bg(cx),
        shown,
        "the 500 ms refresh put the saved theme back over the preview"
    );
}

#[gpui_kit::test]
fn closing_settings_ends_a_preview(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    cx.run_until_parked();
    let saved = painted_bg(cx);
    preview_ember(cx);
    assert_ne!(painted_bg(cx), saved);
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "settings-page"),
        "escape kept settings open"
    );
    assert_eq!(painted_bg(cx), saved, "the preview outlived settings");
}

#[gpui_kit::test]
fn a_hovered_card_previews_until_the_pointer_leaves(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    cx.run_until_parked();
    let saved = painted_bg(cx);
    let on_card = center_of(cx, window, "theme-ember");
    hover_to(cx, window, on_card);
    cx.run_until_parked();
    let shown = painted_bg(cx);
    assert_ne!(shown, saved, "hover did not preview");
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(painted_bg(cx), shown, "the tick ended a hover preview");
    hover_to(cx, window, point(px(2.), px(2.)));
    cx.run_until_parked();
    assert_eq!(painted_bg(cx), saved, "leaving the card did not restore");
}

#[gpui_kit::test]
fn a_saved_font_is_shown_by_its_picker_once_it_is_installed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, |state, _| {
            state.set_installed_fonts_for_test(vec!["Alpha Mono".into(), "Beta Sans".into()]);
        });
    });
    let task = cx.update(|cx| {
        kg_core::Workspace::dispatch(
            Op::Preference(kg_domain::PreferenceOp::Set {
                key: "font-ui".into(),
                value: json!("Beta Sans"),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    cx.run_until_parked();
    assert!(
        present(cx, window, "ui-font"),
        "the UI font picker is not on screen"
    );
    assert_eq!(
        value_of(cx, window, "ui-font").as_deref(),
        Some("Beta Sans")
    );
    assert_eq!(
        value_of(cx, window, "terminal-font").as_deref(),
        Some("Default")
    );
}

#[gpui_kit::test]
fn a_click_keeps_the_previewed_theme_on_screen_before_the_save_lands(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    cx.run_until_parked();
    preview_ember(cx);
    let ember = painted_bg(cx);
    cx.update(|cx| {
        ThemeState::global(cx).update(cx, ThemeState::commit_preview);
    });
    assert_eq!(
        painted_bg(cx),
        ember,
        "the click put the saved theme back on screen before the save"
    );
}
