use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{Color, Op, OpResult, PluginId, PluginOp, ThemeFile, ThemeOp};

use super::{
    THEMES, TIDE, aria_label, get_theme, install_theme_plugin, painted_bg, run_op, set_theme,
    try_op,
};
use crate::tests::kagan::{click, open, open_with_plugins_then, present, selected_of, shows};

#[gpui_kit::test]
fn appearance_shows_a_missing_theme_and_keeps_its_id(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_theme(cx, "ocean");
    click(cx, window, "settings-gear");
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
    click(cx, window, "theme-folder");
    assert!(path.is_dir());
    let url = cx.opened_url().unwrap_or_default();
    assert!(
        url.starts_with("file://") && url.ends_with("/themes"),
        "{url}"
    );
}
