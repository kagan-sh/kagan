"""Metadata decorator for auto-registering API methods as MCP tools.

The ``@expose`` decorator attaches an :class:`ExposeMetadata` dataclass to
an API method so that the auto-registrar can discover it at startup and
generate corresponding MCP tool definitions without hand-authored boilerplate.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

EXPOSE_ATTR = "_kagan_expose"


@dataclass(frozen=True, slots=True)
class ExposeMetadata:
    """Metadata attached to API methods for auto-registration."""

    capability: str
    method: str
    profile: str  # minimum required profile
    mutating: bool
    description: str


def expose(
    capability: str,
    method: str,
    *,
    profile: str = "viewer",
    mutating: bool = False,
    description: str = "",
) -> Any:
    """Decorator marking an API method for auto-registration.

    Args:
        capability: Security capability namespace (e.g. ``"tasks"``).
        method: Method name within the capability (e.g. ``"get"``).
        profile: Minimum :class:`CapabilityProfile` required to invoke the tool.
        mutating: Whether the tool mutates state (affects ToolAnnotations).
        description: Human-readable tool description surfaced by the MCP host.

    Returns:
        A decorator that attaches :class:`ExposeMetadata` to the wrapped function.
    """

    def decorator(fn: Any) -> Any:
        setattr(
            fn,
            EXPOSE_ATTR,
            ExposeMetadata(
                capability=capability,
                method=method,
                profile=profile,
                mutating=mutating,
                description=description,
            ),
        )
        return fn

    return decorator


def collect_exposed_methods(obj: object) -> list[tuple[str, Any, ExposeMetadata]]:
    """Yield ``(method_name, bound_method, metadata)`` for decorated methods on *obj*.

    Inspects all public attributes of *obj* and returns those carrying
    :data:`EXPOSE_ATTR` metadata.
    """
    results: list[tuple[str, Any, ExposeMetadata]] = []
    for name in dir(obj):
        if name.startswith("_"):
            continue
        attr = getattr(obj, name, None)
        if attr is None or not callable(attr):
            continue
        meta = getattr(attr, EXPOSE_ATTR, None)
        if isinstance(meta, ExposeMetadata):
            results.append((name, attr, meta))
    return results


__all__ = ["EXPOSE_ATTR", "ExposeMetadata", "collect_exposed_methods", "expose"]
