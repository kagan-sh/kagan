"""Tests for plugin SDK scaffold contracts and core wiring."""

from __future__ import annotations

import json
import sys
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast

import pytest
from pydantic import ValidationError

from kagan.core.bootstrap import create_app_context
from kagan.core.host import CoreHost
from kagan.core.ipc.contracts import CoreRequest
from kagan.core.plugins.examples import register_example_plugins
from kagan.core.plugins.github import (
    GITHUB_CANONICAL_METHODS,
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_CONTRACT_VERSION,
    GITHUB_PLUGIN_ID,
    RESERVED_GITHUB_CAPABILITY,
    register_github_plugin,
)
from kagan.core.plugins.sdk import (
    JsonPluginManifestLoader,
    PluginManifest,
    PluginOperation,
    PluginRegistrationApi,
    PluginRegistry,
)
from kagan.core.request_dispatch_map import build_request_dispatch_map

if TYPE_CHECKING:
    from pathlib import Path


class _InvalidManifestPlugin:
    manifest = {
        "id": "example.invalid_manifest",
        "name": "Invalid Manifest Plugin",
        "version": "0.1.0",
        "entrypoint": "kagan.core.plugins.examples.invalid:Plugin",
    }

    def register(self, api: PluginRegistrationApi) -> None:
        del api


class _NoOperationPlugin:
    manifest = PluginManifest(
        id="example.no_operation",
        name="No Operation Plugin",
        version="0.1.0",
        entrypoint="kagan.core.plugins.examples.no_operation:Plugin",
        description="Conformance test plugin without operations.",
    )

    def register(self, api: PluginRegistrationApi) -> None:
        del api


class _FailingAfterRegisterPlugin:
    manifest = PluginManifest(
        id="example.rollback",
        name="Rollback Plugin",
        version="0.1.0",
        entrypoint="kagan.core.plugins.examples.rollback:Plugin",
        description="Conformance test plugin to verify rollback semantics.",
    )

    def register(self, api: PluginRegistrationApi) -> None:
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability="plugins",
                method="rollback_probe",
                handler=_rollback_probe,
            )
        )
        msg = "registration failed after operation"
        raise RuntimeError(msg)


class _SimplePlugin:
    def __init__(
        self,
        *,
        plugin_id: str,
        capability: str = "plugins",
        method: str = "noop",
    ) -> None:
        self.manifest = PluginManifest(
            id=plugin_id,
            name=f"{plugin_id} Plugin",
            version="0.1.0",
            entrypoint=f"kagan.core.plugins.examples.{plugin_id}:Plugin",
            description="Simple plugin fixture.",
        )
        self._capability = capability
        self._method = method

    def register(self, api: PluginRegistrationApi) -> None:
        api.register_operation(
            PluginOperation(
                plugin_id=self.manifest.id,
                capability=self._capability,
                method=self._method,
                handler=_rollback_probe,
            )
        )


async def _rollback_probe(ctx: Any, params: dict[str, Any]) -> dict[str, Any]:
    del ctx
    del params
    return {"success": True}


def _write_minimal_config(config_path: Path) -> None:
    config_path.write_text(
        "[general]\nauto_review = false\n"
        'default_worker_agent = "claude"\n\n'
        "[agents.claude]\n"
        'identity = "claude.ai"\nname = "Claude"\nshort_name = "claude"\n'
        'run_command."*" = "echo"\ninteractive_command."*" = "echo"\nactive = true\n',
        encoding="utf-8",
    )


def test_when_manifest_loader_reads_valid_manifest_then_expected_fields_are_loaded(
    tmp_path: Path,
) -> None:
    manifest_path = tmp_path / "plugin.json"
    manifest_path.write_text(
        json.dumps(
            {
                "id": "example.sample",
                "name": "Sample Plugin",
                "version": "0.1.0",
                "entrypoint": "kagan.core.plugins.examples.sample:Plugin",
                "description": "Sample",
            }
        ),
        encoding="utf-8",
    )

    manifest = JsonPluginManifestLoader().load(manifest_path)

    assert manifest.id == "example.sample"
    assert manifest.entrypoint == "kagan.core.plugins.examples.sample:Plugin"


def test_when_manifest_payload_contains_unknown_fields_then_validation_fails() -> None:
    with pytest.raises(ValidationError):
        PluginManifest.model_validate(
            {
                "id": "example.bad",
                "name": "Bad Plugin",
                "version": "0.1.0",
                "entrypoint": "kagan.core.plugins.examples.bad:Plugin",
                "unexpected": "field",
            }
        )


def test_when_registering_example_plugins_then_noop_operation_is_exposed() -> None:
    registry = PluginRegistry()
    register_example_plugins(registry)

    operation = registry.resolve_operation("plugins", "noop_ping")
    assert operation is not None
    assert operation.plugin_id == "example.noop"
    assert operation.mutating is False


def test_when_plugin_manifest_is_not_plugin_manifest_model_then_registration_is_rejected() -> None:
    registry = PluginRegistry()

    with pytest.raises(TypeError, match="Plugin manifest must be an instance of PluginManifest"):
        registry.register_plugin(cast("Any", _InvalidManifestPlugin()))


def test_when_plugin_registers_no_operations_then_registry_rolls_back_manifest() -> None:
    registry = PluginRegistry()

    with pytest.raises(ValueError, match="must register at least one operation"):
        registry.register_plugin(_NoOperationPlugin())

    assert registry.registered_manifests() == ()


def test_when_plugin_registration_raises_after_operation_then_registry_rolls_back_all_changes() -> (
    None
):
    registry = PluginRegistry()

    with pytest.raises(RuntimeError, match="registration failed after operation"):
        registry.register_plugin(_FailingAfterRegisterPlugin())

    assert registry.registered_manifests() == ()
    assert registry.resolve_operation("plugins", "rollback_probe") is None


def test_when_registering_duplicate_plugin_id_then_second_registration_is_rejected() -> None:
    """The registry must keep the first plugin and reject duplicate IDs."""
    registry = PluginRegistry()
    first = _SimplePlugin(plugin_id="example.duplicate")
    second = _SimplePlugin(plugin_id="example.duplicate", method="noop_2")

    registry.register_plugin(first)

    with pytest.raises(ValueError, match="already registered"):
        registry.register_plugin(second)

    manifests = registry.registered_manifests()
    assert len(manifests) == 1
    assert manifests[0].id == "example.duplicate"
    assert registry.resolve_operation("plugins", "noop") is not None
    assert registry.resolve_operation("plugins", "noop_2") is None


def test_when_registering_owned_method_then_registration_is_rejected() -> None:
    """Capability/method ownership must be unique across plugins."""
    registry = PluginRegistry()
    first = _SimplePlugin(plugin_id="example.alpha", method="same_method")
    conflicting = _SimplePlugin(plugin_id="example.beta", method="same_method")

    registry.register_plugin(first)

    with pytest.raises(ValueError, match="already registered by plugin"):
        registry.register_plugin(conflicting)

    assert registry.resolve_operation("plugins", "same_method") is not None
    assert [manifest.id for manifest in registry.registered_manifests()] == ["example.alpha"]


@pytest.mark.asyncio()
async def test_when_core_host_handles_example_plugin_op_then_response_is_successful() -> None:
    registry = PluginRegistry()
    register_example_plugins(registry)

    host = CoreHost()
    host._ctx = cast("Any", SimpleNamespace(plugin_registry=registry))
    host.register_session("maintainer-session", "maintainer")

    response = await host.handle_request(
        CoreRequest(
            session_id="maintainer-session",
            capability="plugins",
            method="noop_ping",
            params={"echo": "hello"},
        )
    )

    assert response.ok
    assert response.result == {
        "success": True,
        "plugin_id": "example.noop",
        "echo": "hello",
    }


@pytest.mark.asyncio()
async def test_when_plugin_policy_hook_denies_request_then_core_host_returns_policy_error() -> None:
    registry = PluginRegistry()
    register_example_plugins(registry)

    host = CoreHost()
    host._ctx = cast("Any", SimpleNamespace(plugin_registry=registry))
    host.register_session("maintainer-session", "maintainer")

    response = await host.handle_request(
        CoreRequest(
            session_id="maintainer-session",
            capability="plugins",
            method="noop_ping",
            params={"disabled": True},
        )
    )

    assert not response.ok
    assert response.error is not None
    assert response.error.code == "PLUGIN_POLICY_DENIED"


@pytest.mark.asyncio()
async def test_when_session_profile_is_below_plugin_minimum_then_core_host_denies_request() -> None:
    registry = PluginRegistry()
    register_example_plugins(registry)

    host = CoreHost()
    host._ctx = cast("Any", SimpleNamespace(plugin_registry=registry))
    host.register_session("viewer-session", "viewer")

    response = await host.handle_request(
        CoreRequest(
            session_id="viewer-session",
            capability="plugins",
            method="noop_ping",
        )
    )

    assert not response.ok
    assert response.error is not None
    assert response.error.code == "AUTHORIZATION_DENIED"


@pytest.mark.asyncio()
async def test_when_create_app_context_initializes_then_example_plugin_operation_is_registered(
    tmp_path: Path,
) -> None:
    config_path = tmp_path / "config.toml"
    db_path = tmp_path / "test.db"
    _write_minimal_config(config_path)

    ctx = await create_app_context(config_path=config_path, db_path=db_path)
    try:
        operation = ctx.plugin_registry.resolve_operation("plugins", "noop_ping")
        assert operation is not None
    finally:
        await ctx.close()


def test_when_registering_github_plugin_then_contract_probe_operation_is_exposed() -> None:
    registry = PluginRegistry()

    register_github_plugin(registry)

    operation = registry.resolve_operation(GITHUB_CAPABILITY, GITHUB_CONTRACT_PROBE_METHOD)
    assert operation is not None
    assert operation.plugin_id == GITHUB_PLUGIN_ID
    assert operation.mutating is False


def test_when_querying_operations_by_method_then_matching_operations_are_returned() -> None:
    registry = PluginRegistry()
    register_github_plugin(registry)

    operations = registry.operations_for_method("validate_review_transition")

    assert len(operations) == 1
    assert operations[0].plugin_id == GITHUB_PLUGIN_ID


def test_when_registering_conflicting_github_probe_then_registry_rejects_collision() -> None:
    registry = PluginRegistry()
    register_github_plugin(registry)
    conflicting = _SimplePlugin(
        plugin_id="example.conflict",
        capability=GITHUB_CAPABILITY,
        method=GITHUB_CONTRACT_PROBE_METHOD,
    )

    with pytest.raises(ValueError, match="already registered by plugin"):
        registry.register_plugin(conflicting)


def test_when_registering_github_plugin_then_probe_has_no_builtin_dispatch_collision() -> None:
    dispatch_map = build_request_dispatch_map()

    assert GITHUB_CAPABILITY != RESERVED_GITHUB_CAPABILITY
    assert (GITHUB_CAPABILITY, GITHUB_CONTRACT_PROBE_METHOD) not in dispatch_map


def test_when_registering_github_plugin_then_handlers_module_is_not_eagerly_imported() -> None:
    handlers_module_name = "kagan.core.plugins.github.entrypoints.plugin_handlers"
    sys.modules.pop(handlers_module_name, None)

    registry = PluginRegistry()
    register_github_plugin(registry)

    assert handlers_module_name not in sys.modules


@pytest.mark.asyncio()
async def test_core_host_github_probe_loads_handlers_and_contract_stable() -> None:
    handlers_module_name = "kagan.core.plugins.github.entrypoints.plugin_handlers"
    sys.modules.pop(handlers_module_name, None)

    registry = PluginRegistry()
    register_github_plugin(registry)

    host = CoreHost()
    host._ctx = cast("Any", SimpleNamespace(plugin_registry=registry))
    host.register_session("maintainer-session", "maintainer")

    response = await host.handle_request(
        CoreRequest(
            session_id="maintainer-session",
            capability=GITHUB_CAPABILITY,
            method=GITHUB_CONTRACT_PROBE_METHOD,
            params={"echo": "hello"},
        )
    )

    assert response.ok
    assert response.result == {
        "success": True,
        "plugin_id": GITHUB_PLUGIN_ID,
        "contract_version": GITHUB_CONTRACT_VERSION,
        "capability": GITHUB_CAPABILITY,
        "method": GITHUB_CONTRACT_PROBE_METHOD,
        "canonical_methods": list(GITHUB_CANONICAL_METHODS),
        "reserved_official_capability": RESERVED_GITHUB_CAPABILITY,
        "echo": "hello",
    }
    assert handlers_module_name in sys.modules


@pytest.mark.asyncio()
async def test_create_app_context_registers_github_probe_without_eager_handlers_import(
    tmp_path: Path,
) -> None:
    handlers_module_name = "kagan.core.plugins.github.entrypoints.plugin_handlers"
    sys.modules.pop(handlers_module_name, None)

    config_path = tmp_path / "config.toml"
    db_path = tmp_path / "test.db"
    _write_minimal_config(config_path)

    ctx = await create_app_context(config_path=config_path, db_path=db_path)
    try:
        operation = ctx.plugin_registry.resolve_operation(
            GITHUB_CAPABILITY, GITHUB_CONTRACT_PROBE_METHOD
        )
        assert operation is not None
        assert operation.plugin_id == GITHUB_PLUGIN_ID
        assert handlers_module_name not in sys.modules
    finally:
        await ctx.close()
