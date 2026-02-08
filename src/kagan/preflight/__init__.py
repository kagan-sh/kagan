"""Pre-flight checks and issue detection for Kagan startup."""

from __future__ import annotations

from .presets import (
    ISSUE_PRESETS,
    DetectedIssue,
    IssuePreset,
    IssueSeverity,
    IssueType,
    PreflightResult,
    create_no_agents_issues,
)
from .resolution import ACPCommandResolution, resolve_acp_command
from .service import detect_issues

__all__ = [
    "ISSUE_PRESETS",
    "ACPCommandResolution",
    "DetectedIssue",
    "IssuePreset",
    "IssueSeverity",
    "IssueType",
    "PreflightResult",
    "create_no_agents_issues",
    "detect_issues",
    "resolve_acp_command",
]
