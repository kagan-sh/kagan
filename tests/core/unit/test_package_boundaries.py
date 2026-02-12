from __future__ import annotations

import ast
from pathlib import Path


def _iter_imports(file_path: Path) -> set[str]:
    tree = ast.parse(file_path.read_text(encoding="utf-8"))
    imports: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.add(alias.name)
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            imports.add(node.module)
    return imports


def _matches_prefix(module: str, prefix: str) -> bool:
    return module == prefix or module.startswith(f"{prefix}.")


def _collect_violations(root: Path, forbidden_prefixes: tuple[str, ...]) -> list[str]:
    violations: list[str] = []
    for file_path in sorted(root.rglob("*.py")):
        imports = _iter_imports(file_path)
        for module in sorted(imports):
            if any(_matches_prefix(module, prefix) for prefix in forbidden_prefixes):
                violations.append(f"{file_path}: {module}")
    return violations


def test_core_does_not_import_tui_or_mcp_packages() -> None:
    repo_root = Path(__file__).resolve().parents[3]
    core_root = repo_root / "src" / "kagan" / "core"

    violations = _collect_violations(core_root, ("kagan.tui", "kagan.mcp", "kagan.cli"))
    assert not violations, "Forbidden core imports:\n" + "\n".join(violations)


def test_tui_does_not_import_mcp_package() -> None:
    repo_root = Path(__file__).resolve().parents[3]
    tui_root = repo_root / "src" / "kagan" / "tui"

    violations = _collect_violations(tui_root, ("kagan.mcp",))
    assert not violations, "Forbidden TUI imports:\n" + "\n".join(violations)


def test_mcp_does_not_import_tui_package() -> None:
    repo_root = Path(__file__).resolve().parents[3]
    mcp_root = repo_root / "src" / "kagan" / "mcp"

    violations = _collect_violations(mcp_root, ("kagan.tui",))
    assert not violations, "Forbidden MCP imports:\n" + "\n".join(violations)
