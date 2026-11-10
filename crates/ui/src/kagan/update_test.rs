use kg_domain::InstallKind;

use super::{COPY, NOTES, Primary, RESTART};

#[test]
fn each_install_kind_has_its_own_primary_action() {
    assert!(matches!(Primary::of(InstallKind::Homebrew), Primary::Copy));
    assert!(matches!(Primary::of(InstallKind::App), Primary::Restart));
    assert!(matches!(
        Primary::of(InstallKind::Installer),
        Primary::Restart
    ));
    assert!(matches!(
        Primary::of(InstallKind::AppImage),
        Primary::Restart
    ));
    assert!(matches!(Primary::of(InstallKind::Other), Primary::Notes));
}

#[test]
fn each_primary_action_has_its_label() {
    assert_eq!(Primary::Copy.label(), COPY);
    assert_eq!(Primary::Restart.label(), RESTART);
    assert_eq!(Primary::Notes.label(), NOTES);
}
