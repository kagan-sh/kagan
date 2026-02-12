"""Plugin SDK scaffold contracts for provider-neutral core extensions."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

from pydantic import BaseModel, ConfigDict, Field

from kagan.core.security import CapabilityProfile

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from kagan.core.bootstrap import AppContext

_PLUGIN_ID_PATTERN = re.compile(r"^[a-z][a-z0-9_.-]{2,63}$")
_CAPABILITY_PATTERN = re.compile(r"^[a-z][a-z0-9_]{1,31}$")
_METHOD_PATTERN = re.compile(r"^[a-z][a-z0-9_]{1,63}$")

_PROFILE_RANK: dict[CapabilityProfile, int] = {
    CapabilityProfile.VIEWER: 0,
    CapabilityProfile.PLANNER: 1,
    CapabilityProfile.PAIR_WORKER: 2,
    CapabilityProfile.OPERATOR: 3,
    CapabilityProfile.MAINTAINER: 4,
}


class PluginManifest(BaseModel):
    """Schema contract for plugin metadata."""

    model_config = ConfigDict(extra="forbid")

    id: str = Field(min_length=3, max_length=64, pattern=r"^[a-z][a-z0-9_.-]{2,63}$")
    name: str = Field(min_length=1, max_length=80)
    version: str = Field(min_length=1, max_length=32)
    entrypoint: str = Field(min_length=1, max_length=200)
    description: str = Field(default="", max_length=280)
    minimum_core_version: str | None = Field(default=None, max_length=32)


@runtime_checkable
class PluginManifestLoader(Protocol):
    """Contract for manifest file loaders."""

    def load(self, manifest_path: Path) -> PluginManifest:
        """Load and validate a plugin manifest from disk."""


class JsonPluginManifestLoader:
    """Default manifest loader for JSON manifests."""

    def load(self, manifest_path: Path) -> PluginManifest:
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
        return PluginManifest.model_validate(payload)


@runtime_checkable
class PluginOperationHandler(Protocol):
    """Callable contract for plugin operation handlers."""

    async def __call__(self, ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
        """Execute plugin operation logic."""


@dataclass(frozen=True, slots=True, kw_only=True)
class PluginOperation:
    """Registration descriptor for one plugin capability/method pair."""

    plugin_id: str
    capability: str
    method: str
    handler: PluginOperationHandler
    minimum_profile: CapabilityProfile = CapabilityProfile.MAINTAINER
    mutating: bool = False
    description: str = ""


@dataclass(frozen=True, slots=True)
class PluginPolicyContext:
    """Request context passed to plugin policy hooks."""

    plugin_id: str
    capability: str
    method: str
    session_id: str
    profile: CapabilityProfile
    params: Mapping[str, Any]


@dataclass(frozen=True, slots=True)
class PluginPolicyDecision:
    """Policy decision returned by plugin policy hooks."""

    allowed: bool
    code: str
    message: str


@runtime_checkable
class PluginPolicyHook(Protocol):
    """Callable contract for plugin policy hooks."""

    def __call__(self, context: PluginPolicyContext) -> PluginPolicyDecision | None:
        """Optionally return a decision for a plugin request."""


@runtime_checkable
class PluginRegistrationApi(Protocol):
    """Core registration API exposed to plugin implementations."""

    def register_operation(self, operation: PluginOperation) -> None:
        """Register one capability/method handler for a plugin."""

    def register_policy_hook(
        self,
        *,
        plugin_id: str,
        capability: str,
        method: str,
        hook: PluginPolicyHook,
    ) -> None:
        """Register a policy hook for a plugin capability/method."""


@runtime_checkable
class Plugin(Protocol):
    """Plugin implementation contract."""

    manifest: PluginManifest

    def register(self, api: PluginRegistrationApi) -> None:
        """Register plugin operations and policy hooks."""


class PluginRegistry(PluginRegistrationApi):
    """In-memory registry for plugin manifests, operations, and policy hooks."""

    def __init__(self) -> None:
        self._manifests: dict[str, PluginManifest] = {}
        self._operations: dict[tuple[str, str], PluginOperation] = {}
        self._policy_hooks: dict[tuple[str, str], list[PluginPolicyHook]] = {}

    def load_manifest(
        self,
        manifest_path: Path,
        *,
        loader: PluginManifestLoader | None = None,
    ) -> PluginManifest:
        """Load and validate a plugin manifest via the provided loader contract."""
        manifest_loader = loader or JsonPluginManifestLoader()
        return manifest_loader.load(manifest_path)

    def register_plugin(self, plugin: Plugin) -> None:
        """Register a plugin manifest and invoke its registration hooks."""
        manifest = plugin.manifest
        if not isinstance(manifest, PluginManifest):
            msg = "Plugin manifest must be an instance of PluginManifest"
            raise TypeError(msg)
        if not _PLUGIN_ID_PATTERN.fullmatch(manifest.id):
            msg = f"Plugin ID '{manifest.id}' must match {_PLUGIN_ID_PATTERN.pattern}"
            raise ValueError(msg)
        if manifest.id in self._manifests:
            msg = f"Plugin '{manifest.id}' is already registered"
            raise ValueError(msg)

        self._manifests[manifest.id] = manifest
        try:
            plugin.register(self)
        except Exception:  # quality-allow-broad-except
            self._rollback_plugin(manifest.id)
            raise
        if self._registered_operation_count(manifest.id) == 0:
            self._rollback_plugin(manifest.id)
            msg = f"Plugin '{manifest.id}' must register at least one operation"
            raise ValueError(msg)

    def register_operation(self, operation: PluginOperation) -> None:
        """Register a capability/method dispatch target for a known plugin."""
        plugin_manifest = self._manifests.get(operation.plugin_id)
        if plugin_manifest is None:
            msg = f"Plugin '{operation.plugin_id}' must be registered before operations"
            raise ValueError(msg)
        if not _PLUGIN_ID_PATTERN.fullmatch(operation.plugin_id):
            msg = f"Plugin ID '{operation.plugin_id}' must match {_PLUGIN_ID_PATTERN.pattern}"
            raise ValueError(msg)
        if not _CAPABILITY_PATTERN.fullmatch(operation.capability):
            msg = f"Capability '{operation.capability}' must match {_CAPABILITY_PATTERN.pattern}"
            raise ValueError(msg)
        if not _METHOD_PATTERN.fullmatch(operation.method):
            msg = f"Method '{operation.method}' must match {_METHOD_PATTERN.pattern}"
            raise ValueError(msg)

        key = (operation.capability, operation.method)
        existing = self._operations.get(key)
        if existing is not None:
            msg = (
                f"Capability '{operation.capability}.{operation.method}' is already registered by "
                f"plugin '{existing.plugin_id}'"
            )
            raise ValueError(msg)
        self._operations[key] = operation

        # Ensure plugin entrypoint metadata stays discoverable for diagnostics.
        self._manifests[plugin_manifest.id] = plugin_manifest

    def register_policy_hook(
        self,
        *,
        plugin_id: str,
        capability: str,
        method: str,
        hook: PluginPolicyHook,
    ) -> None:
        """Register a policy hook for an existing plugin operation."""
        key = (capability, method)
        operation = self._operations.get(key)
        if operation is None:
            msg = f"Cannot register policy hook for unknown operation '{capability}.{method}'"
            raise ValueError(msg)
        if operation.plugin_id != plugin_id:
            msg = (
                f"Operation '{capability}.{method}' belongs to plugin '{operation.plugin_id}', "
                f"not '{plugin_id}'"
            )
            raise ValueError(msg)
        self._policy_hooks.setdefault(key, []).append(hook)

    def registered_manifests(self) -> tuple[PluginManifest, ...]:
        """Return registered manifests sorted by plugin ID."""
        return tuple(self._manifests[key] for key in sorted(self._manifests))

    def resolve_operation(self, capability: str, method: str) -> PluginOperation | None:
        """Resolve a plugin operation by capability/method."""
        return self._operations.get((capability, method))

    def evaluate_policy(
        self,
        *,
        capability: str,
        method: str,
        session_id: str,
        profile: CapabilityProfile,
        params: Mapping[str, Any],
    ) -> PluginPolicyDecision | None:
        """Evaluate plugin-specific authorization for a capability/method.

        Returns ``None`` when no plugin owns the operation.
        """
        operation = self.resolve_operation(capability, method)
        if operation is None:
            return None

        if _PROFILE_RANK[profile] < _PROFILE_RANK[operation.minimum_profile]:
            return PluginPolicyDecision(
                allowed=False,
                code="AUTHORIZATION_DENIED",
                message=(
                    f"Profile '{profile}' is not authorized for plugin operation "
                    f"{capability}.{method}"
                ),
            )

        context = PluginPolicyContext(
            plugin_id=operation.plugin_id,
            capability=capability,
            method=method,
            session_id=session_id,
            profile=profile,
            params=params,
        )
        for hook in self._policy_hooks.get((capability, method), []):
            try:
                decision = hook(context)
            except Exception as exc:  # quality-allow-broad-except
                return PluginPolicyDecision(
                    allowed=False,
                    code="PLUGIN_POLICY_ERROR",
                    message=f"Plugin '{operation.plugin_id}' policy hook failed: {exc}",
                )
            if decision is not None and not decision.allowed:
                return decision

        return PluginPolicyDecision(
            allowed=True,
            code="OK",
            message="Plugin policy allowed request",
        )

    def _rollback_plugin(self, plugin_id: str) -> None:
        self._manifests.pop(plugin_id, None)
        operation_keys = [
            key for key, operation in self._operations.items() if operation.plugin_id == plugin_id
        ]
        for key in operation_keys:
            self._operations.pop(key, None)
            self._policy_hooks.pop(key, None)

    def _registered_operation_count(self, plugin_id: str) -> int:
        return sum(1 for operation in self._operations.values() if operation.plugin_id == plugin_id)


__all__ = [
    "JsonPluginManifestLoader",
    "Plugin",
    "PluginManifest",
    "PluginManifestLoader",
    "PluginOperation",
    "PluginOperationHandler",
    "PluginPolicyContext",
    "PluginPolicyDecision",
    "PluginPolicyHook",
    "PluginRegistrationApi",
    "PluginRegistry",
]
