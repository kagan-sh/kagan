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
    """Result of signal parsing with optional reason and metadata.

    For APPROVE signals, additional context helps maintainers understand the changes:
    - reason/summary: Brief description of what was implemented
    - approach: Technical approach or pattern used (aids debugging)
    - key_files: Primary files to examine when debugging or extending
    """

    signal: Signal
    reason: str = ""
    approach: str = ""
    key_files: str = ""


_PATTERNS = [
    (Signal.COMPLETE, re.compile(r"<complete\s*/?>", re.IGNORECASE)),
    (Signal.BLOCKED, re.compile(r'<blocked\s+reason="([^"]+)"\s*/?>', re.IGNORECASE)),
    (Signal.CONTINUE, re.compile(r"<continue\s*/?>", re.IGNORECASE)),
    (Signal.APPROVE, re.compile(r"<approve\s*[^>]*/?>", re.IGNORECASE)),
    (Signal.REJECT, re.compile(r'<reject\s+reason="([^"]+)"\s*/?>', re.IGNORECASE)),
]


_APPROVE_ATTRS = {
    "summary": re.compile(r'summary="([^"]+)"', re.IGNORECASE),
    "approach": re.compile(r'approach="([^"]+)"', re.IGNORECASE),
    "key_files": re.compile(r'key_files="([^"]+)"', re.IGNORECASE),
}


def parse_signal(output: str) -> SignalResult:
    """Parse agent output for completion signal.

    Args:
        output: Agent response text.

    Returns:
        SignalResult with parsed signal. Defaults to CONTINUE if no signal found.
    """

    signals_with_reason = {Signal.BLOCKED, Signal.REJECT}

    for sig, pat in _PATTERNS:
        if m := pat.search(output):
            if sig == Signal.APPROVE:
                return _parse_approve_signal(m.group(0))
            reason = m.group(1) if sig in signals_with_reason else ""
            return SignalResult(sig, reason)
    return SignalResult(Signal.CONTINUE)


def _parse_approve_signal(approve_tag: str) -> SignalResult:
    """Parse APPROVE signal with optional approach and key_files attributes.

    Args:
        approve_tag: The full <approve .../> tag string.

    Returns:
        SignalResult with summary, approach, and key_files populated.
    """
    summary = ""
    approach = ""
    key_files = ""

    if m := _APPROVE_ATTRS["summary"].search(approve_tag):
        summary = m.group(1)
    if m := _APPROVE_ATTRS["approach"].search(approve_tag):
        approach = m.group(1)
    if m := _APPROVE_ATTRS["key_files"].search(approve_tag):
        key_files = m.group(1)

    return SignalResult(Signal.APPROVE, reason=summary, approach=approach, key_files=key_files)
