"""Shared fixtures for feature tests.

These fixtures extend the main conftest.py fixtures with feature-test-specific setup.
"""

from __future__ import annotations

# Re-export fixtures from parent conftest so they're available in feature tests
# pytest automatically discovers fixtures from conftest.py files in parent directories
