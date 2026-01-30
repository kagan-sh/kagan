"""Terminal output cleaner that processes ANSI escape sequences and control characters.

Adapted from toad's ANSI parsing to provide clean terminal output for display.
Handles carriage returns, escape sequences, and cursor movements to produce
readable text output.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

# ANSI CSI (Control Sequence Introducer) pattern: ESC [ ... final_byte
# Matches sequences like \x1b[0m, \x1b[31;1m, \x1b[2K, etc.
CSI_PATTERN = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")

# OSC (Operating System Command) pattern: ESC ] ... BEL or ESC \
OSC_PATTERN = re.compile(r"\x1b\].*?(?:\x07|\x1b\\)")

# Other escape sequences: ESC followed by a single character
ESCAPE_PATTERN = re.compile(r"\x1b[^\[\]]")


@dataclass
class Line:
    """Represents a single terminal line being built."""

    content: str = ""
    cursor: int = 0

    def write(self, text: str) -> None:
        """Write text at cursor position (replace mode)."""
        if not text:
            return
        before = self.content[: self.cursor]
        after = self.content[self.cursor + len(text) :]
        # Pad with spaces if cursor is beyond current content
        if self.cursor > len(self.content):
            before = self.content + " " * (self.cursor - len(self.content))
            after = ""
        self.content = before + text + after
        self.cursor += len(text)

    def carriage_return(self) -> None:
        """Move cursor to beginning of line."""
        self.cursor = 0

    def backspace(self) -> None:
        """Move cursor back one position."""
        self.cursor = max(0, self.cursor - 1)


@dataclass
class TerminalBuffer:
    """Simple terminal buffer that tracks lines and cursor position."""

    lines: list[Line] = field(default_factory=lambda: [Line()])
    current_line: int = 0

    @property
    def line(self) -> Line:
        """Get current line, creating if necessary."""
        while self.current_line >= len(self.lines):
            self.lines.append(Line())
        return self.lines[self.current_line]

    def newline(self) -> None:
        """Move to next line."""
        self.current_line += 1
        if self.current_line >= len(self.lines):
            self.lines.append(Line())
        self.line.cursor = 0

    def write(self, text: str) -> None:
        """Write text to current line at cursor."""
        self.line.write(text)

    def carriage_return(self) -> None:
        """Handle carriage return."""
        self.line.carriage_return()

    def backspace(self) -> None:
        """Handle backspace."""
        self.line.backspace()

    def cursor_up(self, n: int = 1) -> None:
        """Move cursor up n lines."""
        self.current_line = max(0, self.current_line - n)

    def cursor_down(self, n: int = 1) -> None:
        """Move cursor down n lines."""
        self.current_line += n

    def cursor_to_column(self, col: int) -> None:
        """Move cursor to column (0-based)."""
        self.line.cursor = max(0, col)

    def clear_line_from_cursor(self) -> None:
        """Clear from cursor to end of line."""
        self.line.content = self.line.content[: self.line.cursor]

    def clear_line_to_cursor(self) -> None:
        """Clear from start of line to cursor."""
        self.line.content = " " * self.line.cursor + self.line.content[self.line.cursor :]

    def clear_entire_line(self) -> None:
        """Clear entire line."""
        self.line.content = ""
        self.line.cursor = 0

    def get_output(self) -> str:
        """Get the final output as text."""
        return "\n".join(line.content.rstrip() for line in self.lines)


def _parse_csi_param(param_str: str, default: int = 1) -> int:
    """Parse CSI parameter, returning default if empty."""
    return int(param_str) if param_str.isdigit() else default


def clean_terminal_output(text: str) -> str:
    """Process terminal output to handle control characters and escape sequences.

    Simulates terminal behavior for:
    - Carriage returns (\\r) - moves cursor to line start
    - Newlines (\\n) - moves to next line
    - Backspace (\\x08) - moves cursor back
    - ANSI CSI sequences for cursor movement and line clearing
    - Strips color/style sequences

    Args:
        text: Raw terminal output that may contain control characters.

    Returns:
        Clean text with control characters processed.
    """
    if not text:
        return ""

    buffer = TerminalBuffer()
    pos = 0

    while pos < len(text):
        char = text[pos]

        # Handle control characters
        if char == "\r":
            buffer.carriage_return()
            pos += 1
            continue

        if char == "\n":
            buffer.newline()
            pos += 1
            continue

        if char == "\x08":  # Backspace
            buffer.backspace()
            pos += 1
            continue

        # Handle escape sequences
        if char == "\x1b" and pos + 1 < len(text):
            next_char = text[pos + 1]

            # CSI sequence
            if next_char == "[":
                match = CSI_PATTERN.match(text, pos)
                if match:
                    seq = match.group()
                    _handle_csi(buffer, seq)
                    pos = match.end()
                    continue

            # OSC sequence
            if next_char == "]":
                match = OSC_PATTERN.match(text, pos)
                if match:
                    pos = match.end()
                    continue

            # Other escape sequences (skip)
            pos += 2
            continue

        # Regular character - write to buffer
        # Collect consecutive regular characters for efficiency
        end = pos
        while end < len(text) and text[end] not in "\r\n\x08\x1b":
            end += 1

        if end > pos:
            buffer.write(text[pos:end])
            pos = end
        else:
            pos += 1

    return buffer.get_output()


def _handle_csi(buffer: TerminalBuffer, seq: str) -> None:
    """Handle a CSI escape sequence."""
    # Extract the final character and parameters
    if len(seq) < 3:
        return

    final = seq[-1]
    params = seq[2:-1]  # Skip \x1b[ and final character

    match final:
        case "A":  # Cursor up
            buffer.cursor_up(_parse_csi_param(params))
        case "B":  # Cursor down
            buffer.cursor_down(_parse_csi_param(params))
        case "C":  # Cursor forward
            buffer.line.cursor += _parse_csi_param(params)
        case "D":  # Cursor back
            buffer.line.cursor = max(0, buffer.line.cursor - _parse_csi_param(params))
        case "G":  # Cursor horizontal absolute (1-based)
            buffer.cursor_to_column(_parse_csi_param(params) - 1)
        case "K":  # Erase in line
            param = _parse_csi_param(params, 0)
            if param == 0:
                buffer.clear_line_from_cursor()
            elif param == 1:
                buffer.clear_line_to_cursor()
            elif param == 2:
                buffer.clear_entire_line()
        case "J":  # Erase in display (simplified - just handle current line)
            param = _parse_csi_param(params, 0)
            if param == 0:
                buffer.clear_line_from_cursor()
            elif param == 2:
                buffer.clear_entire_line()
        case "m":
            # SGR (Select Graphics Rendition) - colors/styles - ignore
            pass
        case "H" | "f":
            # Cursor position - parse row;col
            parts = params.split(";")
            if len(parts) >= 2:
                row = _parse_csi_param(parts[0]) - 1
                col = _parse_csi_param(parts[1]) - 1
                buffer.current_line = max(0, row)
                while buffer.current_line >= len(buffer.lines):
                    buffer.lines.append(Line())
                buffer.cursor_to_column(col)
            elif len(parts) == 1 and parts[0]:
                buffer.current_line = max(0, _parse_csi_param(parts[0]) - 1)
                buffer.cursor_to_column(0)
        # Other sequences are ignored (colors, styles, etc.)
