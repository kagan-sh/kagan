"""Provider-neutral no-op plugin used to validate SDK scaffold wiring."""

from __future__ import annotations

from typing import Any

from kagan.core.plugins.sdk import (
    PluginManifest,
    PluginOperation,
    PluginPolicyContext,
    PluginPolicyDecision,
    PluginRegistrationApi,
    PluginRegistry,
)
from kagan.core.security import CapabilityProfile


class NoOpExamplePlugin:
    """Minimal plugin that returns a no-op payload for scaffold validation."""

    manifest = PluginManifest(
        id="example.noop",
        name="Example No-op Plugin",
        version="0.1.0",
        entrypoint="kagan.core.plugins.examples.noop:NoOpExamplePlugin",
        description="Provider-neutral no-op plugin for scaffold validation only.",
    )

    def register(self, api: PluginRegistrationApi) -> None:
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability="plugins",
                method="noop_ping",
                handler=_noop_ping,
                minimum_profile=CapabilityProfile.MAINTAINER,
                mutating=False,
                description="No-op operation for plugin wiring validation.",
            )
        )
        api.register_policy_hook(
            plugin_id=self.manifest.id,
            capability="plugins",
            method="noop_ping",
            hook=_noop_policy_hook,
        )


async def _noop_ping(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    del ctx
    return {
        "success": True,
        "plugin_id": NoOpExamplePlugin.manifest.id,
        "echo": params.get("echo"),
    }


def _noop_policy_hook(context: PluginPolicyContext) -> PluginPolicyDecision | None:
    if context.params.get("disabled") is True:
        return PluginPolicyDecision(
            allowed=False,
            code="PLUGIN_POLICY_DENIED",
            message=f"Plugin '{context.plugin_id}' denied request because disabled=true",
        )
    return None


def register_example_plugins(registry: PluginRegistry) -> None:
    """Register built-in SDK validation plugin(s)."""
    registry.register_plugin(NoOpExamplePlugin())


__all__ = ["NoOpExamplePlugin", "register_example_plugins"]
