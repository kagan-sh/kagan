"""Schema-driven plugin UI API mixin.

This mixin provides two methods used by the TUI:
- ``plugin_ui_catalog``: return declarative UI contributions for allowlisted plugins.
- ``plugin_ui_invoke``: invoke an allowlisted plugin action by action_id.

The core remains the orchestration brain; the TUI is a thin renderer and input
collector. The invoke path is resolved server-side from plugin-provided schema
to avoid trusting any client-supplied operation references.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.debug_log import log as debug_log
from kagan.core.plugins.sdk import PLUGIN_UI_DESCRIBE_METHOD, PluginPolicyDecision
from kagan.core.plugins.ui_schema import UiCatalog, sanitize_plugin_ui_payload
from kagan.core.request_context import get_request_context
from kagan.core.security import CapabilityProfile

if TYPE_CHECKING:
    from collections.abc import Mapping

    from kagan.core.bootstrap import AppContext
    from kagan.core.plugins.sdk import PluginOperation, PluginRegistry


_DEFAULT_REFRESH_FALSE = {"repo": False, "tasks": False, "sessions": False}


def _non_empty_str(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    cleaned = value.strip()
    return cleaned if cleaned else None


def _merge_catalogs(target: UiCatalog, incoming: UiCatalog, *, plugin_id: str) -> UiCatalog:
    diagnostics = list(target.diagnostics)
    diagnostics.extend(f"{plugin_id}: {msg}" for msg in incoming.diagnostics)
    return UiCatalog(
        schema_version=target.schema_version,
        actions=[*target.actions, *incoming.actions],
        forms=[*target.forms, *incoming.forms],
        badges=[*target.badges, *incoming.badges],
        diagnostics=diagnostics,
    )


def _sanitize_refresh(value: object) -> dict[str, bool] | None:
    if not isinstance(value, dict):
        return None
    normalized: dict[str, bool] = {}
    for key in ("repo", "tasks", "sessions"):
        item = value.get(key)
        if isinstance(item, bool):
            normalized[key] = item
    return normalized


def _required_fields_from_form(form: Mapping[str, Any]) -> list[str]:
    fields = form.get("fields")
    if not isinstance(fields, list):
        return []
    required: list[str] = []
    for field in fields:
        if not isinstance(field, dict):
            continue
        name = field.get("name")
        if not isinstance(name, str) or not name.strip():
            continue
        if field.get("required") is True:
            required.append(name)
    return required


def _has_required_value(value: object) -> bool:
    if value is None:
        return False
    if isinstance(value, bool):
        return True
    if isinstance(value, (int, float)):
        return True
    if isinstance(value, str):
        return bool(value.strip())
    return True


def _resolve_session_identity() -> tuple[str, CapabilityProfile]:
    ctx = get_request_context()
    if ctx is None:
        return ("local", CapabilityProfile.MAINTAINER)
    return (ctx.request.session_id, CapabilityProfile(ctx.binding.policy.profile))


class PluginUiApiMixin:
    """Mixin providing schema-driven plugin UI methods for TUI clients."""

    _ctx: AppContext

    def _plugin_registry(self) -> PluginRegistry:
        plugin_registry = getattr(self._ctx, "plugin_registry", None)
        if plugin_registry is None:
            raise RuntimeError("Plugin registry is not initialized")
        return plugin_registry

    def _plugin_ui_allowlist(self) -> set[str] | None:
        """Return the plugin UI allowlist, or ``None`` if all plugins are allowed.

        An empty configured list means "allow everything" (no restriction).
        """
        allowlist = getattr(self._ctx.config.ui, "tui_plugin_ui_allowlist", None)
        if not isinstance(allowlist, list):
            return None
        entries = {item for item in allowlist if isinstance(item, str) and item.strip()}
        return entries if entries else None

    def _evaluate_policy(
        self,
        registry: PluginRegistry,
        *,
        capability: str,
        method: str,
        params: Mapping[str, Any],
    ) -> PluginPolicyDecision | None:
        session_id, profile = _resolve_session_identity()
        return registry.evaluate_policy(
            capability=capability,
            method=method,
            session_id=session_id,
            profile=profile,
            params=params,
        )

    async def plugin_ui_catalog(
        self,
        *,
        project_id: str,
        repo_id: str | None = None,
    ) -> dict[str, Any]:
        cleaned_project_id, cleaned_repo_id = self._clean_project_repo_args(project_id, repo_id)

        allowlist = self._plugin_ui_allowlist()

        registry = self._plugin_registry()
        catalog = UiCatalog(schema_version="1", actions=[], forms=[], badges=[], diagnostics=[])
        params: dict[str, Any] = {"project_id": cleaned_project_id}
        if cleaned_repo_id is not None:
            params["repo_id"] = cleaned_repo_id

        for operation in registry.operations_for_method(PLUGIN_UI_DESCRIBE_METHOD):
            if allowlist is not None and operation.plugin_id not in allowlist:
                continue
            if operation.mutating:
                catalog = _merge_catalogs(
                    catalog,
                    UiCatalog(
                        schema_version="1",
                        actions=[],
                        forms=[],
                        badges=[],
                        diagnostics=["ui_describe must be non-mutating"],
                    ),
                    plugin_id=operation.plugin_id,
                )
                continue
            decision = self._evaluate_policy(
                registry,
                capability=operation.capability,
                method=operation.method,
                params=params,
            )
            if decision is not None and not decision.allowed:
                catalog = _merge_catalogs(
                    catalog,
                    UiCatalog(
                        schema_version="1",
                        actions=[],
                        forms=[],
                        badges=[],
                        diagnostics=[f"policy denied: {decision.code}"],
                    ),
                    plugin_id=operation.plugin_id,
                )
                continue
            result = await operation.handler(self._ctx, params)
            sanitized = sanitize_plugin_ui_payload(result, plugin_id=operation.plugin_id)
            if sanitized.diagnostics:
                debug_log.debug(
                    "[PluginUI] ui_describe diagnostics",
                    plugin=operation.plugin_id,
                    diagnostics=sanitized.diagnostics,
                )
            catalog = _merge_catalogs(catalog, sanitized, plugin_id=operation.plugin_id)

        response: dict[str, Any] = {
            "schema_version": catalog.schema_version,
            "actions": catalog.actions,
            "forms": catalog.forms,
            "badges": catalog.badges,
        }
        if catalog.diagnostics:
            response["diagnostics"] = catalog.diagnostics
        return response

    async def plugin_ui_invoke(
        self,
        *,
        project_id: str,
        plugin_id: str,
        action_id: str,
        repo_id: str | None = None,
        inputs: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        cleaned_project_id, cleaned_repo_id = self._clean_project_repo_args(project_id, repo_id)
        cleaned_plugin_id = _non_empty_str(plugin_id)
        cleaned_action_id = _non_empty_str(action_id)
        if cleaned_plugin_id is None:
            raise ValueError("plugin_id is required")
        if cleaned_action_id is None:
            raise ValueError("action_id is required")

        allowlist = self._plugin_ui_allowlist()
        if allowlist is not None and cleaned_plugin_id not in allowlist:
            raise ValueError(f"Plugin '{cleaned_plugin_id}' is not allowlisted for TUI UI")

        if inputs is not None and not isinstance(inputs, dict):
            raise ValueError("inputs must be an object when provided")
        normalized_inputs = dict(inputs or {})

        registry = self._plugin_registry()
        describe_ops: list[PluginOperation] = [
            op
            for op in registry.operations_for_method(PLUGIN_UI_DESCRIBE_METHOD)
            if op.plugin_id == cleaned_plugin_id
        ]
        if not describe_ops:
            raise ValueError(f"Plugin '{cleaned_plugin_id}' did not register ui_describe")
        if len(describe_ops) > 1:
            raise RuntimeError(f"Plugin '{cleaned_plugin_id}' registered multiple ui_describe ops")
        describe_op = describe_ops[0]

        describe_params: dict[str, Any] = {"project_id": cleaned_project_id}
        if cleaned_repo_id is not None:
            describe_params["repo_id"] = cleaned_repo_id

        decision = self._evaluate_policy(
            registry,
            capability=describe_op.capability,
            method=describe_op.method,
            params=describe_params,
        )
        if decision is not None and not decision.allowed:
            return {
                "ok": False,
                "code": decision.code,
                "message": decision.message,
                "data": None,
                "refresh": dict(_DEFAULT_REFRESH_FALSE),
            }

        describe_payload = await describe_op.handler(self._ctx, describe_params)
        catalog = sanitize_plugin_ui_payload(describe_payload, plugin_id=cleaned_plugin_id)
        action = next(
            (item for item in catalog.actions if item.get("action_id") == cleaned_action_id),
            None,
        )
        if action is None:
            raise ValueError(f"Unknown plugin action: {cleaned_plugin_id}.{cleaned_action_id}")

        form_id = action.get("form_id")
        if isinstance(form_id, str) and form_id.strip():
            form = next(
                (
                    item
                    for item in catalog.forms
                    if item.get("form_id") == form_id and item.get("plugin_id") == cleaned_plugin_id
                ),
                None,
            )
            if form is None:
                raise ValueError(f"Action references unknown form_id: {form_id}")
            required_fields = _required_fields_from_form(form)
            effective_inputs = dict(normalized_inputs)
            effective_inputs.setdefault("project_id", cleaned_project_id)
            if cleaned_repo_id is not None:
                effective_inputs.setdefault("repo_id", cleaned_repo_id)
            missing = [
                name
                for name in required_fields
                if not _has_required_value(effective_inputs.get(name))
            ]
            if missing:
                missing_str = ", ".join(missing)
                raise ValueError(f"Missing required input field(s): {missing_str}")

        operation = action.get("operation")
        if not isinstance(operation, dict):
            raise ValueError("Action operation must be an object")
        capability = _non_empty_str(operation.get("capability"))
        method = _non_empty_str(operation.get("method"))
        if capability is None or method is None:
            raise ValueError("Action operation requires capability and method")

        plugin_operation = registry.resolve_operation(capability, method)
        if plugin_operation is None:
            return {
                "ok": False,
                "code": "PLUGIN_OPERATION_NOT_FOUND",
                "message": f"Plugin operation is not registered: {capability}.{method}",
                "data": None,
                "refresh": dict(_DEFAULT_REFRESH_FALSE),
            }
        if plugin_operation.plugin_id != cleaned_plugin_id:
            return {
                "ok": False,
                "code": "PLUGIN_OPERATION_MISMATCH",
                "message": "Action operation belongs to a different plugin",
                "data": None,
                "refresh": dict(_DEFAULT_REFRESH_FALSE),
            }

        invoke_params: dict[str, Any] = {"project_id": cleaned_project_id}
        if cleaned_repo_id is not None:
            invoke_params["repo_id"] = cleaned_repo_id
        for key in ("project_id", "plugin_id", "action_id"):
            normalized_inputs.pop(key, None)
        if "repo_id" in invoke_params:
            normalized_inputs.pop("repo_id", None)
        invoke_params.update(normalized_inputs)

        invoke_decision = self._evaluate_policy(
            registry,
            capability=plugin_operation.capability,
            method=plugin_operation.method,
            params=invoke_params,
        )
        if invoke_decision is not None and not invoke_decision.allowed:
            return {
                "ok": False,
                "code": invoke_decision.code,
                "message": invoke_decision.message,
                "data": None,
                "refresh": dict(_DEFAULT_REFRESH_FALSE),
            }

        try:
            result = await plugin_operation.handler(self._ctx, invoke_params)
        except Exception as exc:  # quality-allow-broad-except
            return {
                "ok": False,
                "code": "PLUGIN_HANDLER_ERROR",
                "message": f"Plugin handler failed: {exc}",
                "data": None,
                "refresh": dict(_DEFAULT_REFRESH_FALSE),
            }

        if not isinstance(result, dict):
            return {
                "ok": False,
                "code": "PLUGIN_INVALID_RESULT",
                "message": "Plugin handler returned invalid payload",
                "data": None,
                "refresh": dict(_DEFAULT_REFRESH_FALSE),
            }

        ok: bool
        code: str
        message: str
        if isinstance(result.get("success"), bool):
            ok = bool(result.get("success"))
            code = str(result.get("code") or ("OK" if ok else "PLUGIN_ERROR"))
            message = str(result.get("message") or ("OK" if ok else "Plugin operation failed"))
        else:
            ok = True
            code = "OK"
            message = "OK"

        default_refresh = dict(_DEFAULT_REFRESH_FALSE)
        if plugin_operation.mutating:
            default_refresh.update({"repo": True, "tasks": True})
        refresh_override = _sanitize_refresh(result.get("refresh"))
        if refresh_override:
            default_refresh.update(refresh_override)

        data = dict(result)
        data.pop("refresh", None)
        return {
            "ok": ok,
            "code": code,
            "message": message,
            "data": data,
            "refresh": default_refresh,
        }

    @staticmethod
    def _clean_project_repo_args(project_id: str, repo_id: str | None) -> tuple[str, str | None]:
        cleaned_project_id = project_id.strip()
        if not cleaned_project_id:
            raise ValueError("project_id is required")

        cleaned_repo_id: str | None = None
        if repo_id is not None:
            normalized_repo_id = repo_id.strip()
            if not normalized_repo_id:
                raise ValueError("repo_id must be a non-empty string when provided")
            cleaned_repo_id = normalized_repo_id
        return cleaned_project_id, cleaned_repo_id


__all__ = ["PluginUiApiMixin"]
