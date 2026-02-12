"""Path-based marker policy for package/type assignment and xdist grouping."""

from __future__ import annotations

import pytest

_PACKAGE_MARKERS = frozenset({"core", "mcp", "tui"})
_TYPE_MARKERS = frozenset({"unit", "contract", "snapshot", "smoke"})
_DEPRECATED_TEST_MARKERS = frozenset({"integration"})
_DISALLOWED_EXPLICIT_MARKERS = _PACKAGE_MARKERS | _TYPE_MARKERS | _DEPRECATED_TEST_MARKERS
_PATH_MARKER_RULES: tuple[tuple[str, tuple[str, str]], ...] = (
    ("tests/core/unit/", ("core", "unit")),
    ("tests/core/smoke/", ("core", "smoke")),
    ("tests/mcp/contract/", ("mcp", "contract")),
    ("tests/mcp/smoke/", ("mcp", "smoke")),
    ("tests/tui/snapshot/", ("tui", "snapshot")),
    ("tests/tui/smoke/", ("tui", "smoke")),
)


def _normalized_item_path(item: pytest.Item) -> str:
    path_obj = getattr(item, "path", None)
    if path_obj is not None:
        return str(path_obj).replace("\\", "/")
    return str(item.fspath).replace("\\", "/")


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    """Apply package/type markers by path and keep snapshot tests on one worker."""
    del config
    violations: list[str] = []

    for item in items:
        path = _normalized_item_path(item)
        existing = {marker.name for marker in item.iter_markers()}
        explicit = {marker.name for marker in item.own_markers}

        disallowed_explicit = explicit & _DISALLOWED_EXPLICIT_MARKERS
        if disallowed_explicit:
            violations.append(
                f"{item.nodeid}: remove explicit marker(s) {sorted(disallowed_explicit)}; "
                "path-based policy auto-applies package/type markers"
            )
            continue

        required: tuple[str, str] | None = None
        for prefix, markers in _PATH_MARKER_RULES:
            if prefix in path:
                required = markers
                break

        if required is not None:
            expected_package = required[0]
            conflicting = (_PACKAGE_MARKERS & existing) - {expected_package}
            if conflicting:
                violations.append(
                    f"{item.nodeid}: package marker conflict "
                    f"(expected '{expected_package}', found {sorted(conflicting)})"
                )

            for marker_name in required:
                if marker_name not in existing:
                    item.add_marker(getattr(pytest.mark, marker_name))

        if item.get_closest_marker("snapshot"):
            item.add_marker(pytest.mark.xdist_group("snapshots"))
        if item.get_closest_marker("smoke"):
            package = next(
                (name for name in ("core", "mcp", "tui") if item.get_closest_marker(name)),
                None,
            )
            if package is not None:
                item.add_marker(pytest.mark.xdist_group(f"{package}-smoke"))

    if violations:
        rendered = "\n".join(violations[:20])
        if len(violations) > 20:
            rendered = f"{rendered}\n... and {len(violations) - 20} more"
        raise pytest.UsageError(f"Package marker/path mismatch detected:\n{rendered}")
