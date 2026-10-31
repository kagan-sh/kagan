use gpui_kit::Modifiers;
use kg_core::{Motion, Mouse};

/// A button a program can hear about. The wheel's steps are buttons too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Press,
    Release,
    Move,
}

/// A cell on the view, counted from the top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Spot {
    pub(crate) column: u16,
    pub(crate) row: u16,
}

impl Button {
    fn code(self) -> u16 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
            Self::WheelUp => 64,
            Self::WheelDown => 65,
        }
    }
}

/// The bytes that report a mouse event to a program that asked for them. `button` is the one
/// pressed, or held while a move happens. `None` means the program does not want this event, or
/// the legacy form cannot hold the cell.
pub(crate) fn report(
    button: Option<Button>,
    phase: Phase,
    spot: Spot,
    mods: Modifiers,
    mouse: Mouse,
) -> Option<Vec<u8>> {
    let mut code = match phase {
        // The legacy form does not say which button was let go.
        Phase::Release if !mouse.sgr => 3,
        Phase::Press | Phase::Release => button?.code(),
        Phase::Move => {
            if mouse.motion == Motion::Click || (button.is_none() && mouse.motion == Motion::Drag) {
                return None;
            }
            // Motion adds 32 to the button held, or 3 (no button) when none is.
            button.map_or(3, Button::code) + 32
        }
    };
    if mods.alt {
        code += 8;
    }
    if mods.control {
        code += 16;
    }
    let (column, row) = (spot.column + 1, spot.row + 1);
    if mouse.sgr {
        let last = if phase == Phase::Release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{column};{row}{last}").into_bytes());
    }
    // The legacy form stores each number as one byte after 32, so a cell past 222 has no code.
    // The bytes go to the program as they are: a byte above 127 is not text.
    let byte = |value: u16| u8::try_from(value + 32).ok();
    Some(vec![
        0x1b,
        b'[',
        b'M',
        byte(code)?,
        byte(column)?,
        byte(row)?,
    ])
}

#[cfg(test)]
mod tests;
