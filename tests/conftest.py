"""Shared pytest bootstrap for Kagan tests."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from hypothesis import Phase, Verbosity, settings

from tests.helpers.fixtures.isolation import apply_test_env

pytest_plugins = [
    "tests.helpers.fixtures.core",
    "tests.helpers.fixtures.tui",
    "tests.helpers.fixtures.agents",
    "tests.helpers.fixtures.markers",
    "tests.helpers.fixtures.safety",
]

_WORKSPACE_ROOT = Path(__file__).resolve().parents[1]
_SRC = str(_WORKSPACE_ROOT / "src")
if _SRC in sys.path:
    sys.path.remove(_SRC)
sys.path.insert(0, _SRC)
os.environ["PYTHONPATH"] = _SRC

_WORKSPACE_SRC_ROOTS = (_WORKSPACE_ROOT / "src",)


def _is_workspace_module(module: object) -> bool:
    module_file = getattr(module, "__file__", None)
    if not module_file:
        return True
    module_path = Path(module_file).resolve()
    return any(module_path.is_relative_to(root) for root in _WORKSPACE_SRC_ROOTS)


_stale_kagan_modules: list[str] = []
_stale_schema_loaded = False
for _module_name, _module in list(sys.modules.items()):
    if not (_module_name == "kagan" or _module_name.startswith("kagan.")):
        continue
    if _module is not None and _is_workspace_module(_module):
        continue
    _stale_kagan_modules.append(_module_name)
    if _module_name == "kagan.core.adapters.db.schema":
        _stale_schema_loaded = True

for _module_name in _stale_kagan_modules:
    del sys.modules[_module_name]

if _stale_schema_loaded:
    import sqlmodel.main as sqlmodel_main

    sqlmodel_main.default_registry.dispose()
    sqlmodel_main.default_registry.metadata.clear()

apply_test_env()

settings.register_profile(
    "ci",
    max_examples=100,
    deadline=None,
    phases=[Phase.explicit, Phase.reuse, Phase.generate, Phase.shrink],
)
settings.register_profile(
    "dev",
    max_examples=20,
    deadline=500,
)
settings.register_profile(
    "debug",
    max_examples=10,
    verbosity=Verbosity.verbose,
    deadline=None,
)
settings.load_profile(os.getenv("HYPOTHESIS_PROFILE", "dev"))
