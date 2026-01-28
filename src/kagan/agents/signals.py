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
    APPROVE = "approve"
    REJECT = "reject"


@dataclass
class SignalResult:
    """Result of signal parsing with optional reason."""

    signal: Signal
    reason: str = ""


# Patterns for parsing signals from agent output
_PATTERNS = [
    (Signal.COMPLETE, re.compile(r"<complete\s*/?>", re.IGNORECASE)),
    (Signal.BLOCKED, re.compile(r'<blocked\s+reason="([^"]+)"\s*/?>', re.IGNORECASE)),
    (Signal.CONTINUE, re.compile(r"<continue\s*/?>", re.IGNORECASE)),
    (Signal.APPROVE, re.compile(r'<approve\s+summary="([^"]+)"\s*/?>', re.IGNORECASE)),
    (Signal.REJECT, re.compile(r'<reject\s+reason="([^"]+)"\s*/?>', re.IGNORECASE)),
]


def parse_signal(output: str) -> SignalResult:
    """Parse agent output for completion signal.

    Args:
        output: Agent response text.

    Returns:
        SignalResult with parsed signal. Defaults to CONTINUE if no signal found.
    """
    # Signals that capture a reason/summary in group(1)
    signals_with_reason = {Signal.BLOCKED, Signal.APPROVE, Signal.REJECT}

    for sig, pat in _PATTERNS:
        if m := pat.search(output):
            reason = m.group(1) if sig in signals_with_reason else ""
            return SignalResult(sig, reason)
    return SignalResult(Signal.CONTINUE)
