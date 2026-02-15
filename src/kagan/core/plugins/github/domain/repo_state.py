"""Typed adapters for GitHub plugin state persisted in Repo.scripts."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.gh_adapter import (
    GITHUB_CONNECTION_KEY,
    load_connection_metadata,
    resolve_connection_repo_name,
)
from kagan.core.plugins.github.sync import (
    GITHUB_ISSUE_MAPPING_KEY,
    GITHUB_LEASE_ENFORCEMENT_KEY,
    GITHUB_SYNC_CHECKPOINT_KEY,
    GITHUB_TASK_PR_MAPPING_KEY,
    IssueMapping,
    SyncCheckpoint,
    TaskPRMapping,
    load_checkpoint,
    load_lease_enforcement,
    load_mapping,
    load_repo_default_mode,
    load_task_pr_mapping,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from kagan.core.models.enums import TaskType


@dataclass(frozen=True, slots=True)
class ConnectionState:
    """Normalized connection metadata state."""

    raw_value: object | None
    normalized: dict[str, Any] | None
    needs_rewrite: bool


def load_connection_state(scripts: Mapping[str, object] | None) -> ConnectionState:
    """Load and normalize connection metadata from persisted repo scripts."""
    raw_value = scripts.get(GITHUB_CONNECTION_KEY) if scripts else None
    parsed_value = raw_value
    if isinstance(raw_value, str):
        try:
            parsed_value = json.loads(raw_value)
        except json.JSONDecodeError:
            parsed_value = None

    normalized = load_connection_metadata(parsed_value)
    needs_rewrite = (
        isinstance(parsed_value, dict) and normalized is not None and parsed_value != normalized
    )
    return ConnectionState(
        raw_value=raw_value,
        normalized=normalized,
        needs_rewrite=needs_rewrite,
    )


def resolve_owner_repo(connection: Mapping[str, Any]) -> tuple[str, str] | None:
    """Resolve owner/repo tuple from normalized connection metadata."""
    owner = str(connection.get("owner") or "").strip()
    repo_name = resolve_connection_repo_name(connection)
    if not owner or not repo_name:
        return None
    return owner, repo_name


def load_sync_checkpoint_state(scripts: Mapping[str, object] | None) -> SyncCheckpoint:
    """Load sync checkpoint from persisted repo scripts."""
    return load_checkpoint(scripts)


def load_issue_mapping_state(scripts: Mapping[str, object] | None) -> IssueMapping:
    """Load issue mapping from persisted repo scripts."""
    return load_mapping(scripts)


def load_repo_default_mode_state(scripts: Mapping[str, object] | None) -> TaskType | None:
    """Load repo default task mode from persisted repo scripts."""
    return load_repo_default_mode(scripts)


def load_lease_enforcement_state(scripts: Mapping[str, object] | None) -> bool:
    """Load repo-level lease enforcement policy from persisted repo scripts."""
    return load_lease_enforcement(scripts)


def load_pr_mapping_state(scripts: Mapping[str, object] | None) -> TaskPRMapping:
    """Load PR mapping from persisted repo scripts."""
    return load_task_pr_mapping(scripts)


def encode_connection_update(connection_metadata: Mapping[str, Any]) -> dict[str, str]:
    """Encode connection metadata update payload for repo script storage."""
    return {GITHUB_CONNECTION_KEY: json.dumps(dict(connection_metadata))}


def encode_sync_state_update(
    checkpoint: SyncCheckpoint,
    mapping: IssueMapping,
) -> dict[str, str]:
    """Encode sync checkpoint and issue mapping update payload."""
    return {
        GITHUB_SYNC_CHECKPOINT_KEY: json.dumps(checkpoint.to_dict()),
        GITHUB_ISSUE_MAPPING_KEY: json.dumps(mapping.to_dict()),
    }


def encode_pr_mapping_update(mapping: TaskPRMapping) -> dict[str, str]:
    """Encode task-to-PR mapping update payload."""
    return {GITHUB_TASK_PR_MAPPING_KEY: json.dumps(mapping.to_dict())}


def encode_lease_enforcement_update(enabled: bool) -> dict[str, str]:
    """Encode lease enforcement policy update payload."""
    return {GITHUB_LEASE_ENFORCEMENT_KEY: json.dumps(enabled)}


__all__ = [
    "ConnectionState",
    "encode_connection_update",
    "encode_lease_enforcement_update",
    "encode_pr_mapping_update",
    "encode_sync_state_update",
    "load_connection_state",
    "load_issue_mapping_state",
    "load_lease_enforcement_state",
    "load_pr_mapping_state",
    "load_repo_default_mode_state",
    "load_sync_checkpoint_state",
    "resolve_owner_repo",
]
