use kg_core::Ansi;

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
