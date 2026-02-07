"""Snapshot tests for Kagan TUI.

This package provides infrastructure for real E2E snapshot tests that:
- Only mock the agent CLI (Claude Code) - no actual AI running
- Everything else is real: git, filesystem, database
- Assert keyboard sequences yield approved snapshots
"""
