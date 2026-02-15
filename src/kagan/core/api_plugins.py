"""Generic plugin API mixin for capability/method dispatch."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


class PluginApiMixin:
    """Mixin providing generic plugin operation dispatch.

    Expects ``self._ctx`` to be an :class:`AppContext` instance,
    initialised by :class:`KaganAPI.__init__`.
    """

    _ctx: AppContext

    async def invoke_plugin(
        self,
        capability: str,
        method: str,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Invoke a registered plugin operation by capability and method.

        Args:
            capability: Plugin capability namespace.
            method: Operation method name.
            params: Optional parameters dict.

        Returns:
            Plugin operation result dict.

        Raises:
            RuntimeError: If plugin registry or operation is not available.
        """
        plugin_registry = getattr(self._ctx, "plugin_registry", None)
        if plugin_registry is None:
            raise RuntimeError("Plugin registry is not initialized")

        operation = plugin_registry.resolve_operation(capability, method)
        if operation is None:
            msg = f"Plugin operation not registered: {capability}.{method}"
            raise RuntimeError(msg)

        result = await operation.handler(self._ctx, params or {})
        if not isinstance(result, dict):
            msg = f"Plugin operation returned invalid payload: {capability}.{method}"
            raise RuntimeError(msg)
        return result


__all__ = ["PluginApiMixin"]
