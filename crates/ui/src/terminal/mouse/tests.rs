use gpui_kit::Modifiers;
use kg_core::{Motion, Mouse};

use super::{Button, Phase, Spot, report};

fn at(column: u16, row: u16) -> Spot {
    Spot { column, row }
}

fn plain() -> Modifiers {
    Modifiers::default()
}

fn wire(motion: Motion) -> Mouse {
    Mouse { motion, sgr: true }
}

fn legacy(motion: Motion) -> Mouse {
    Mouse { motion, sgr: false }
}

#[test]
fn a_press_and_a_release_use_the_wire_form_the_program_asked_for() {
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(2, 1),
            plain(),
            wire(Motion::Click)
        ),
        Some(b"\x1b[<0;3;2M".to_vec())
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Release,
            at(2, 1),
            plain(),
            wire(Motion::Click)
        ),
        Some(b"\x1b[<0;3;2m".to_vec())
    );
    // Legacy adds 32 to each number, and a release says only "button 3".
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(2, 1),
            plain(),
            legacy(Motion::Click)
        ),
        Some(b"\x1b[M #\"".to_vec())
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Release,
            at(2, 1),
            plain(),
            legacy(Motion::Click)
        ),
        Some(b"\x1b[M##\"".to_vec())
    );
}

#[test]
fn the_wheel_is_a_button_that_is_only_pressed() {
    assert_eq!(
        report(
            Some(Button::WheelUp),
            Phase::Press,
            at(0, 0),
            plain(),
            wire(Motion::Click)
        ),
        Some(b"\x1b[<64;1;1M".to_vec())
    );
    assert_eq!(
        report(
            Some(Button::WheelDown),
            Phase::Press,
            at(0, 0),
            plain(),
            legacy(Motion::Click)
        ),
        Some(b"\x1b[Ma!!".to_vec())
    );
    assert_eq!(
        report(
            Some(Button::WheelUp),
            Phase::Press,
            at(0, 0),
            plain(),
            legacy(Motion::Click)
        ),
        Some(b"\x1b[M`!!".to_vec())
    );
}

#[test]
fn moves_follow_what_the_program_asked_for() {
    let spot = at(0, 0);
    // Clicks only: no moves at all.
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Move,
            spot,
            plain(),
            wire(Motion::Click)
        ),
        None
    );
    // Drags: moves only while a button is held.
    assert_eq!(
        report(None, Phase::Move, spot, plain(), wire(Motion::Drag)),
        None
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Move,
            spot,
            plain(),
            wire(Motion::Drag)
        ),
        Some(b"\x1b[<32;1;1M".to_vec())
    );
    // All moves: with no button held, the code is 3 + 32.
    assert_eq!(
        report(None, Phase::Move, spot, plain(), wire(Motion::All)),
        Some(b"\x1b[<35;1;1M".to_vec())
    );
}

#[test]
fn alt_and_control_add_their_bits() {
    let alt = Modifiers {
        alt: true,
        ..plain()
    };
    let both = Modifiers {
        alt: true,
        control: true,
        ..plain()
    };
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(0, 0),
            alt,
            wire(Motion::Click)
        ),
        Some(b"\x1b[<8;1;1M".to_vec())
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(0, 0),
            both,
            wire(Motion::Click)
        ),
        Some(b"\x1b[<24;1;1M".to_vec())
    );
}

#[test]
fn the_legacy_form_has_no_code_for_a_cell_past_222() {
    assert!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(222, 0),
            plain(),
            legacy(Motion::Click)
        )
        .is_some()
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(223, 0),
            plain(),
            legacy(Motion::Click)
        ),
        None
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(223, 0),
            plain(),
            wire(Motion::Click)
        ),
        Some(b"\x1b[<0;224;1M".to_vec())
    );
}

#[test]
fn a_column_from_96_is_one_byte_not_text() {
    // One-based column 96 is 96 + 32 = 128, which is the byte 0x80: one byte, not UTF-8.
    let column_95 = report(
        Some(Button::Left),
        Phase::Press,
        at(94, 0),
        plain(),
        legacy(Motion::Click),
    );
    assert_eq!(column_95, Some(vec![0x1b, b'[', b'M', 32, 127, 33]));
    let column_96 = report(
        Some(Button::Left),
        Phase::Press,
        at(95, 0),
        plain(),
        legacy(Motion::Click),
    );
    assert_eq!(column_96, Some(vec![0x1b, b'[', b'M', 32, 128, 33]));
}

#[test]
fn the_legacy_limit_is_one_byte_255_and_sgr_goes_past_it() {
    let at_limit = report(
        Some(Button::Left),
        Phase::Press,
        at(222, 0),
        plain(),
        legacy(Motion::Click),
    );
    assert_eq!(at_limit, Some(vec![0x1b, b'[', b'M', 32, 255, 33]));
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(223, 0),
            plain(),
            legacy(Motion::Click)
        ),
        None
    );
    assert_eq!(
        report(
            Some(Button::Left),
            Phase::Press,
            at(223, 0),
            plain(),
            wire(Motion::Click)
        ),
        Some(b"\x1b[<0;224;1M".to_vec())
    );
}
