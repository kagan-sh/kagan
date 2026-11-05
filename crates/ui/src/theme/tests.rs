use gpui_kit::component::highlighter::HighlightTheme;
use kg_core::Ansi;

use super::colors::highlight;
use super::*;

const NAMES: [Ansi; 16] = [
    Ansi::Black,
    Ansi::Red,
    Ansi::Green,
    Ansi::Yellow,
    Ansi::Blue,
    Ansi::Magenta,
    Ansi::Cyan,
    Ansi::White,
    Ansi::BrightBlack,
    Ansi::BrightRed,
    Ansi::BrightGreen,
    Ansi::BrightYellow,
    Ansi::BrightBlue,
    Ansi::BrightMagenta,
    Ansi::BrightCyan,
    Ansi::BrightWhite,
];

fn catalog_file(id: &str) -> ThemeFile {
    let catalog = builtins();
    let entry = catalog.iter().find(|entry| entry.id == id).unwrap();
    entry.file.clone().unwrap()
}

#[test]
fn the_built_in_themes_all_load() {
    let catalog = builtins();
    let ids: Vec<&str> = catalog.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["fjord", "ember", "grey"]);
    for entry in &catalog {
        assert!(
            entry.file.is_ok(),
            "{}: {:?}",
            entry.id,
            entry.file.as_ref().err()
        );
    }
}

#[test]
fn fjord_keeps_its_dark_terminal_in_both_modes() {
    let fjord = fjord();
    let light = palette(&fjord, ThemeMode::Light).terminal();
    assert_eq!(light.bg, Color::new(0x1a_1d24));
    assert_eq!(light.fg, Color::new(0xe5_e9f0));
    assert_eq!(light, palette(&fjord, ThemeMode::Dark).terminal());
}

#[test]
fn fjord_terminal_colors_are_all_different() {
    let mut seen = Vec::new();
    for name in NAMES {
        let color = terminal::ansi(&FJORD, name);
        assert!(!seen.contains(&color), "{name:?} repeats a color");
        seen.push(color);
    }
    assert_ne!(FJORD.bg, FJORD.black, "black is the background tone");
    assert_ne!(FJORD.fg, FJORD.white);
}

#[test]
fn a_theme_without_a_terminal_keeps_the_xterm_colors() {
    let ember = palette(&catalog_file("ember"), ThemeMode::Dark).terminal();
    assert_eq!(ember.bg, Color::new(0x17_1614));
    assert_eq!(ember.fg, Color::new(0xec_e8e0));
    assert_eq!(ember.cursor, Color::new(0xec_e8e0));
    assert_eq!(terminal::ansi(&ember, Ansi::Red), Color::new(0xcd_0000));
    assert_eq!(
        terminal::ansi(&ember, Ansi::BrightBlue),
        Color::new(0x5c_5cff)
    );
}

#[test]
fn a_folder_file_cannot_replace_a_built_in_theme() {
    let dir = tempfile::tempdir().unwrap();
    let ocean = include_str!("../../../../docs/themes/ocean.json");
    std::fs::write(dir.path().join("ember.json"), ocean).unwrap();
    std::fs::write(dir.path().join("fjord.json"), ocean).unwrap();
    std::fs::write(dir.path().join("ocean.json"), ocean).unwrap();
    let user = read_user(dir.path());
    let ids: Vec<&str> = user.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["ocean"]);
    assert_eq!(user[0].source, Source::User);
}

#[test]
fn a_written_theme_reads_back_the_same() {
    let fjord = fjord();
    let text = serde_json::to_vec_pretty(&fjord).unwrap();
    assert_eq!(ThemeFile::parse(&text).unwrap(), fjord);
    let ocean = include_bytes!("../../../../docs/themes/ocean.json");
    let ocean = ThemeFile::parse(ocean).unwrap();
    let text = serde_json::to_vec_pretty(&ocean).unwrap();
    assert_eq!(ThemeFile::parse(&text).unwrap(), ocean);
}

#[test]
fn a_default_look_projects_the_numbers_the_window_had_before() {
    let config = config(&fjord(), ThemeMode::Light);
    assert_eq!(config.font_size, Some(16.));
    assert_eq!(config.mono_font_size, Some(12.));
    assert_eq!(config.radius, Some(8));
    assert_eq!(config.radius_lg, Some(12));
    assert_eq!(config.colors.selection.as_deref(), Some("#d8dee7"));
    assert_eq!(config.colors.input.as_deref(), Some("#d3d9e2"));
}

#[test]
fn a_custom_look_projects_its_numbers() {
    let mut file = fjord();
    file.look = kg_domain::Look {
        radius: kg_domain::Radius {
            xs: 0,
            sm: 0,
            md: 2,
            lg: 4,
        },
        type_sizes: kg_domain::TypeSizes {
            caption: 10,
            small: 11,
            body: 12,
            title: 14,
            mono: 11,
        },
        row: 24,
        control: 20,
        ..kg_domain::Look::default()
    };
    let config = config(&file, ThemeMode::Dark);
    assert_eq!(config.mono_font_size, Some(11.));
    assert_eq!(file.look.radius.xs, 0);
    assert_eq!(config.radius, Some(2));
    assert_eq!(config.radius_lg, Some(4));
}

#[test]
fn a_theme_without_highlight_keeps_the_gpui_default_syntax() {
    let file = fjord();
    assert_eq!(
        *highlight(&file, ThemeMode::Light),
        *HighlightTheme::default_light()
    );
    assert_eq!(
        *highlight(&file, ThemeMode::Dark),
        *HighlightTheme::default_dark()
    );
}

#[test]
fn a_theme_picks_its_own_highlight_for_each_mode() {
    let mut file = fjord();
    file.highlight = Some(kg_domain::Highlight {
        light: Some(serde_json::json!({
            "name": "Light ink",
            "appearance": "light",
            "style": {"syntax": {"keyword": {"color": "#112233", "font_weight": 700}}},
        })),
        dark: Some(serde_json::json!({
            "name": "Dark ink",
            "style": {"syntax": {"keyword": {"color": "#445566"}}},
        })),
    });
    let light = highlight(&file, ThemeMode::Light);
    assert_eq!(light.name, "Light ink");
    assert_eq!(
        light
            .style
            .syntax
            .style("keyword")
            .and_then(|style| style.color),
        Some(token_color(Color::new(0x11_2233)))
    );
    let dark = highlight(&file, ThemeMode::Dark);
    assert_eq!(dark.name, "Dark ink");
    assert_eq!(
        dark.style
            .syntax
            .style("keyword")
            .and_then(|style| style.color),
        Some(token_color(Color::new(0x44_5566)))
    );
    file.highlight.as_mut().unwrap().light = None;
    assert_eq!(
        *highlight(&file, ThemeMode::Light),
        *HighlightTheme::default_light(),
        "a mode without its own syntax keeps the default"
    );
}
