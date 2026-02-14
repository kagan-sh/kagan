"""Bundled official GitHub plugin scaffold and registration entrypoint."""

from __future__ import annotations

from importlib import import_module
from typing import Any, cast

from kagan.core.plugins.github.contract import (
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
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


async def _contract_probe(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    del ctx
    runtime_module = import_module("kagan.core.plugins.github.runtime")
    build_contract_probe_payload = cast("Any", runtime_module).build_contract_probe_payload
    return cast("dict[str, Any]", build_contract_probe_payload(params))


def register_github_plugin(registry: PluginRegistry) -> None:
    """Register bundled official GitHub plugin operations."""
    registry.register_plugin(GitHubPlugin())


__all__ = ["GitHubPlugin", "register_github_plugin"]
