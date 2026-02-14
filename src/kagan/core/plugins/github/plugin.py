"""Bundled official GitHub plugin scaffold and registration entrypoint."""

from __future__ import annotations

from importlib import import_module
from typing import Any, cast

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
    GITHUB_PLUGIN_ID,
)
from kagan.core.plugins.sdk import (
    PluginManifest,
    PluginOperation,
    PluginRegistrationApi,
    PluginRegistry,
)
from kagan.core.security import CapabilityProfile


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


async def _contract_probe(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    del ctx
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    build_contract_probe_payload = cast("Any", runtime_module).build_contract_probe_payload
    return cast("dict[str, Any]", build_contract_probe_payload(params))


async def _connect_repo(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_connect_repo = cast("Any", runtime_module).handle_connect_repo
    return cast("dict[str, Any]", await handle_connect_repo(ctx, params))


async def _sync_issues(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_sync_issues = cast("Any", runtime_module).handle_sync_issues
    return cast("dict[str, Any]", await handle_sync_issues(ctx, params))


async def _acquire_lease(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_acquire_lease = cast("Any", runtime_module).handle_acquire_lease
    return cast("dict[str, Any]", await handle_acquire_lease(ctx, params))


async def _release_lease(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_release_lease = cast("Any", runtime_module).handle_release_lease
    return cast("dict[str, Any]", await handle_release_lease(ctx, params))


async def _get_lease_state(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_get_lease_state = cast("Any", runtime_module).handle_get_lease_state
    return cast("dict[str, Any]", await handle_get_lease_state(ctx, params))


async def _create_pr_for_task(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_create_pr_for_task = cast("Any", runtime_module).handle_create_pr_for_task
    return cast("dict[str, Any]", await handle_create_pr_for_task(ctx, params))


async def _link_pr_to_task(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_link_pr_to_task = cast("Any", runtime_module).handle_link_pr_to_task
    return cast("dict[str, Any]", await handle_link_pr_to_task(ctx, params))


async def _reconcile_pr_status(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    handle_reconcile_pr_status = cast("Any", runtime_module).handle_reconcile_pr_status
    return cast("dict[str, Any]", await handle_reconcile_pr_status(ctx, params))


def register_github_plugin(registry: PluginRegistry) -> None:
    """Register bundled official GitHub plugin operations."""
    registry.register_plugin(GitHubPlugin())


__all__ = ["GitHubPlugin", "register_github_plugin"]
