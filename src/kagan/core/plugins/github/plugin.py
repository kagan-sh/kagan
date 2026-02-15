"""Bundled official GitHub plugin scaffold and registration entrypoint."""

from __future__ import annotations

from importlib import import_module
from typing import Any

from kagan.core.plugins.github.contract import (
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_METHOD_ACQUIRE_LEASE,
    GITHUB_METHOD_CONNECT_REPO,
    GITHUB_METHOD_CREATE_PR_FOR_TASK,
    GITHUB_METHOD_GET_LEASE_STATE,
    GITHUB_METHOD_LINK_PR_TO_TASK,
    GITHUB_METHOD_RECONCILE_PR_STATUS,
    GITHUB_METHOD_RELEASE_LEASE,
    GITHUB_METHOD_SYNC_ISSUES,
    GITHUB_METHOD_VALIDATE_REVIEW_TRANSITION,
    GITHUB_PLUGIN_ID,
)
from kagan.core.plugins.sdk import (
    PLUGIN_UI_DESCRIBE_METHOD,
    PluginManifest,
    PluginOperation,
    PluginRegistrationApi,
    PluginRegistry,
)
from kagan.core.security import CapabilityProfile


def _plugin_handlers_module() -> Any:
    """Load handler module lazily to avoid eager plugin side effects."""
    return import_module("kagan.core.plugins.github.entrypoints.plugin_handlers")


def _make_handler_dispatch(
    handler_name: str,
    *,
    include_ctx: bool,
) -> Any:
    async def _dispatch(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
        handlers_module = _plugin_handlers_module()
        handler = getattr(handlers_module, handler_name)
        if not include_ctx:
            del ctx
            return handler(params)
        return await handler(ctx, params)

    return _dispatch


_contract_probe = _make_handler_dispatch("build_contract_probe_payload", include_ctx=False)
_connect_repo = _make_handler_dispatch("handle_connect_repo", include_ctx=True)
_sync_issues = _make_handler_dispatch("handle_sync_issues", include_ctx=True)
_acquire_lease = _make_handler_dispatch("handle_acquire_lease", include_ctx=True)
_release_lease = _make_handler_dispatch("handle_release_lease", include_ctx=True)
_get_lease_state = _make_handler_dispatch("handle_get_lease_state", include_ctx=True)
_create_pr_for_task = _make_handler_dispatch("handle_create_pr_for_task", include_ctx=True)
_link_pr_to_task = _make_handler_dispatch("handle_link_pr_to_task", include_ctx=True)
_reconcile_pr_status = _make_handler_dispatch("handle_reconcile_pr_status", include_ctx=True)
_validate_review_transition = _make_handler_dispatch(
    "handle_validate_review_transition",
    include_ctx=True,
)
_ui_describe = _make_handler_dispatch("handle_ui_describe", include_ctx=True)


class GitHubPlugin:
    """Official bundled GitHub plugin scaffold with a stable contract probe."""

    manifest = PluginManifest(
        id=GITHUB_PLUGIN_ID,
        name="Official GitHub Plugin",
        version="0.1.0",
        entrypoint="kagan.core.plugins.github.plugin:GitHubPlugin",
        description="Bundled GitHub plugin scaffold with stable contract probe semantics.",
    )

    def register(self, api: PluginRegistrationApi) -> None:
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_CONTRACT_PROBE_METHOD,
                handler=_contract_probe,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=False,
                description="Return the canonical GitHub plugin operation contract.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_CONNECT_REPO,
                handler=_connect_repo,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Connect a repo to GitHub with preflight checks.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_SYNC_ISSUES,
                handler=_sync_issues,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Sync GitHub issues to Kagan task projections.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_ACQUIRE_LEASE,
                handler=_acquire_lease,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Acquire a lease on a GitHub issue for the current instance.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_RELEASE_LEASE,
                handler=_release_lease,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Release a lease on a GitHub issue.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_GET_LEASE_STATE,
                handler=_get_lease_state,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=False,
                description="Get the current lease state for a GitHub issue.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_CREATE_PR_FOR_TASK,
                handler=_create_pr_for_task,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Create a PR for a task and link it.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_LINK_PR_TO_TASK,
                handler=_link_pr_to_task,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Link an existing PR to a task.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_RECONCILE_PR_STATUS,
                handler=_reconcile_pr_status,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=True,
                description="Reconcile the PR status for a task from GitHub.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=GITHUB_METHOD_VALIDATE_REVIEW_TRANSITION,
                handler=_validate_review_transition,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=False,
                description="Validate REVIEW transition guardrails for GitHub-connected repos.",
            )
        )
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=GITHUB_CAPABILITY,
                method=PLUGIN_UI_DESCRIBE_METHOD,
                handler=_ui_describe,
                minimum_profile=CapabilityProfile.VIEWER,
                mutating=False,
                description=(
                    "Provide declarative TUI UI schema contributions for GitHub operations."
                ),
            )
        )


def register_github_plugin(registry: PluginRegistry) -> None:
    """Register bundled official GitHub plugin operations."""
    registry.register_plugin(GitHubPlugin())


__all__ = ["GitHubPlugin", "register_github_plugin"]
