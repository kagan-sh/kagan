"""Shared settings allowlist and normalization for MCP/admin updates."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.core.builtin_agents import BUILTIN_AGENTS
from kagan.core.config import WORKTREE_BASE_REF_STRATEGY_VALUES
from kagan.core.models.enums import VALID_PAIR_BACKENDS

if TYPE_CHECKING:
    from collections.abc import Mapping

    from kagan.core.config import KaganConfig

EXPOSED_SETTINGS: tuple[str, ...] = (
    "general.auto_review",
    "general.auto_approve",
    "general.require_review_approval",
    "general.serialize_merges",
    "general.default_base_branch",
    "general.auto_sync_base_branch",
    "general.worktree_base_ref_strategy",
    "general.max_concurrent_agents",
    "general.default_worker_agent",
    "general.default_pair_terminal_backend",
    "general.default_model_claude",
    "general.default_model_opencode",
    "general.default_model_codex",
    "general.default_model_gemini",
    "general.default_model_kimi",
    "general.default_model_copilot",
    "general.tasks_wait_default_timeout_seconds",
    "general.tasks_wait_max_timeout_seconds",
    "ui.skip_pair_instructions",
)

_BOOL_FIELDS: set[str] = {
    "general.auto_review",
    "general.auto_approve",
    "general.require_review_approval",
    "general.serialize_merges",
    "general.auto_sync_base_branch",
    "ui.skip_pair_instructions",
}
_TIMEOUT_SECONDS_FIELDS: set[str] = {
    "general.tasks_wait_default_timeout_seconds",
    "general.tasks_wait_max_timeout_seconds",
}
_OPTIONAL_MODEL_FIELDS: set[str] = {
    "general.default_model_claude",
    "general.default_model_opencode",
    "general.default_model_codex",
    "general.default_model_gemini",
    "general.default_model_kimi",
    "general.default_model_copilot",
}
_WORKER_AGENTS: set[str] = set(BUILTIN_AGENTS.keys())


def exposed_settings_snapshot(config: KaganConfig) -> dict[str, object]:
    """Return MCP-safe settings snapshot as dotted-path keys."""
    return {
        "general.auto_review": config.general.auto_review,
        "general.auto_approve": config.general.auto_approve,
        "general.require_review_approval": config.general.require_review_approval,
        "general.serialize_merges": config.general.serialize_merges,
        "general.default_base_branch": config.general.default_base_branch,
        "general.auto_sync_base_branch": config.general.auto_sync_base_branch,
        "general.worktree_base_ref_strategy": config.general.worktree_base_ref_strategy,
        "general.max_concurrent_agents": config.general.max_concurrent_agents,
        "general.default_worker_agent": config.general.default_worker_agent,
        "general.default_pair_terminal_backend": config.general.default_pair_terminal_backend,
        "general.default_model_claude": config.general.default_model_claude,
        "general.default_model_opencode": config.general.default_model_opencode,
        "general.default_model_codex": config.general.default_model_codex,
        "general.default_model_gemini": config.general.default_model_gemini,
        "general.default_model_kimi": config.general.default_model_kimi,
        "general.default_model_copilot": config.general.default_model_copilot,
        "general.tasks_wait_default_timeout_seconds": (
            config.general.tasks_wait_default_timeout_seconds
        ),
        "general.tasks_wait_max_timeout_seconds": (config.general.tasks_wait_max_timeout_seconds),
        "ui.skip_pair_instructions": config.ui.skip_pair_instructions,
    }


def normalize_settings_updates(fields: Mapping[str, object]) -> dict[str, object]:
    """Validate and normalize an update payload for MCP settings mutation."""
    unknown = sorted(key for key in fields if key not in EXPOSED_SETTINGS)
    if unknown:
        unknown_keys = ", ".join(unknown)
        raise ValueError(f"Unsupported settings field(s): {unknown_keys}")

    normalized: dict[str, object] = {}
    for key, value in fields.items():
        normalized[key] = _normalize_value(key, value)
    return normalized


def _normalize_value(key: str, value: object) -> object:
    if key in _BOOL_FIELDS:
        if isinstance(value, bool):
            return value
        raise ValueError(f"{key} must be a boolean")

    if key == "general.default_base_branch":
        return _normalize_non_empty_string(key, value)

    if key == "general.worktree_base_ref_strategy":
        strategy = _normalize_non_empty_string(key, value)
        if strategy not in WORKTREE_BASE_REF_STRATEGY_VALUES:
            options = ", ".join(sorted(WORKTREE_BASE_REF_STRATEGY_VALUES))
            raise ValueError(f"general.worktree_base_ref_strategy must be one of: {options}")
        return strategy

    if key == "general.max_concurrent_agents":
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError("general.max_concurrent_agents must be an integer")
        if value < 1 or value > 10:
            raise ValueError("general.max_concurrent_agents must be between 1 and 10")
        return value

    if key in _TIMEOUT_SECONDS_FIELDS:
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError(f"{key} must be a positive integer")
        if value < 1 or value > 3600:
            raise ValueError(f"{key} must be between 1 and 3600")
        return value

    if key == "general.default_worker_agent":
        agent = _normalize_non_empty_string(key, value)
        if agent not in _WORKER_AGENTS:
            options = ", ".join(sorted(_WORKER_AGENTS))
            raise ValueError(f"general.default_worker_agent must be one of: {options}")
        return agent

    if key == "general.default_pair_terminal_backend":
        backend = _normalize_non_empty_string(key, value).lower()
        if backend not in VALID_PAIR_BACKENDS:
            options = ", ".join(sorted(VALID_PAIR_BACKENDS))
            raise ValueError(f"general.default_pair_terminal_backend must be one of: {options}")
        return backend

    if key in _OPTIONAL_MODEL_FIELDS:
        if value is None:
            return None
        if isinstance(value, str):
            cleaned = value.strip()
            return cleaned or None
        raise ValueError(f"{key} must be a string or null")

    raise ValueError(f"Unsupported settings field: {key}")


def _normalize_non_empty_string(field: str, value: object) -> str:
    if not isinstance(value, str):
        raise ValueError(f"{field} must be a string")
    cleaned = value.strip()
    if not cleaned:
        raise ValueError(f"{field} cannot be empty")
    return cleaned


__all__ = [
    "EXPOSED_SETTINGS",
    "exposed_settings_snapshot",
    "normalize_settings_updates",
]
