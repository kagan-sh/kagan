/// Whether `c` cannot sit in a name: a control character, or a bidi override or isolate, which
/// reverses the text drawn after it so the screen shows something other than what is stored.
#[must_use]
pub fn unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}
