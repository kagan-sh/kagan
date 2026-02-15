"""Strict canonical metadata tests for GitHub connection payloads."""

from __future__ import annotations

from kagan.core.plugins.github.gh_adapter import (
    load_connection_metadata,
    resolve_connection_repo_name,
)


def test_resolve_connection_repo_name_ignores_legacy_name_key() -> None:
    assert resolve_connection_repo_name({"name": "legacy-repo"}) == ""


def test_load_connection_metadata_rejects_legacy_name_only_payload() -> None:
    metadata = {
        "host": "github.com",
        "owner": "acme",
        "name": "legacy-repo",
    }

    assert load_connection_metadata(metadata) is None


def test_load_connection_metadata_requires_canonical_repo_key_in_json_string() -> None:
    raw = '{"host":"github.com","owner":"acme","name":"legacy-repo"}'

    assert load_connection_metadata(raw) is None


def test_load_connection_metadata_accepts_canonical_repo_and_drops_legacy_name() -> None:
    metadata = {
        "host": "github.com",
        "owner": "acme",
        "repo": " repo-a ",
        "name": "legacy-repo",
    }

    loaded = load_connection_metadata(metadata)

    assert loaded is not None
    assert loaded["repo"] == "repo-a"
    assert "name" not in loaded
