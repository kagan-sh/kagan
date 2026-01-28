"""Parse agent completion signals from output."""

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import Enum


class Signal(Enum):
    """Agent completion signal types."""

    CONTINUE = "continue"
    COMPLETE = "complete"
    BLOCKED = "blocked"


@dataclass
class SignalResult:
    """Result of signal parsing with optional reason."""

    signal: Signal
    reason: str = ""

    def __eq__(self, other: object) -> bool:
        if isinstance(other, SignalResult):
            return self.signal == other.signal and self.reason == other.reason
        return NotImplemented


# Patterns for parsing signals from agent output
_PATTERNS = [
    (Signal.COMPLETE, re.compile(r"<complete\s*/?>", re.IGNORECASE)),
    (Signal.BLOCKED, re.compile(r'<blocked\s+reason="([^"]+)"\s*/?>', re.IGNORECASE)),
    (Signal.CONTINUE, re.compile(r"<continue\s*/?>", re.IGNORECASE)),
]


def parse_signal(output: str) -> SignalResult:
    """Parse agent output for completion signal.

    Args:
        output: Agent response text.

    Returns:
        SignalResult with parsed signal. Defaults to CONTINUE if no signal found.
    """
    for sig, pat in _PATTERNS:
        if m := pat.search(output):
            return SignalResult(sig, m.group(1) if sig == Signal.BLOCKED else "")
    return SignalResult(Signal.CONTINUE)
