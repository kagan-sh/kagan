"""Autouse safety fixtures for isolated test storage and production-path guards."""

from __future__ import annotations

import shutil
import sqlite3
from typing import TYPE_CHECKING, Any, cast

import pytest

from tests.helpers.fixtures.isolation import (
    REAL_PRODUCTION_DB_PATH,
    REAL_PRODUCTION_LOCKS_DIR,
    TEST_BASE_DIR,
    TEST_ENV,
    coerce_db_path,
    is_production_db_path,
    is_within_test_base,
)

if TYPE_CHECKING:
    from collections.abc import Generator
    from os import PathLike
    from pathlib import Path

    from sqlalchemy import URL


@pytest.fixture(scope="session", autouse=True)
def _cleanup_test_base_dir() -> Generator[None, None, None]:
    """Remove the shared test path override directory after the session."""
    yield
    shutil.rmtree(TEST_BASE_DIR, ignore_errors=True)


@pytest.fixture(scope="session", autouse=True)
def _assert_default_paths_are_isolated() -> None:
    """Verify default Kagan storage paths resolve inside the test base directory."""
    from kagan.core.instance_lock import _get_locks_dir
    from kagan.core.paths import get_cache_dir, get_config_dir, get_data_dir, get_database_path

    default_paths = (
        get_data_dir(),
        get_config_dir(),
        get_cache_dir(),
        get_database_path(),
        _get_locks_dir(),
    )
    for path in default_paths:
        if not is_within_test_base(path):
            raise AssertionError(f"Default test path escaped isolated storage root: {path}")


@pytest.fixture(autouse=True)
def _enforce_isolated_storage_env(monkeypatch: pytest.MonkeyPatch) -> None:
    """Force all Kagan/XDG paths to stay in an isolated test directory."""
    for key, value in TEST_ENV.items():
        monkeypatch.setenv(key, value)


@pytest.fixture(autouse=True)
def _block_production_kagan_db_access(monkeypatch: pytest.MonkeyPatch) -> None:
    """Fail fast if any test tries to touch a real installed Kagan database."""
    from kagan.core import instance_lock as instance_lock_module
    from kagan.core.adapters.db import engine as db_engine

    original_create_async_engine = db_engine.create_async_engine
    original_sqlite_connect = sqlite3.connect
    original_get_locks_dir = instance_lock_module._get_locks_dir

    def _assert_test_db(database: object) -> None:
        db_path = coerce_db_path(database)
        if db_path is not None and is_production_db_path(db_path):
            raise AssertionError(
                f"Tests must not access production Kagan DB path: {REAL_PRODUCTION_DB_PATH}"
            )

    def _guarded_get_locks_dir() -> Path:
        locks_dir = original_get_locks_dir()
        if locks_dir.expanduser().resolve() == REAL_PRODUCTION_LOCKS_DIR:
            raise AssertionError(
                f"Tests must not access production Kagan lock path: {REAL_PRODUCTION_LOCKS_DIR}"
            )
        return locks_dir

    def _guarded_create_async_engine(url: URL | str, *args: object, **kwargs: object):
        _assert_test_db(url)
        create_engine = cast("Any", original_create_async_engine)
        return create_engine(url, *args, **kwargs)

    def _guarded_sqlite_connect(
        database: str | bytes | PathLike[str] | PathLike[bytes],
        *args: object,
        **kwargs: object,
    ):
        _assert_test_db(database)
        connect = cast("Any", original_sqlite_connect)
        return connect(database, *args, **kwargs)

    monkeypatch.setattr(db_engine, "create_async_engine", _guarded_create_async_engine)
    monkeypatch.setattr(sqlite3, "connect", _guarded_sqlite_connect)
    monkeypatch.setattr(instance_lock_module, "_get_locks_dir", _guarded_get_locks_dir)


@pytest.fixture(autouse=True)
def _mock_platform_system(monkeypatch: pytest.MonkeyPatch, request: pytest.FixtureRequest) -> None:
    """Handle @pytest.mark.mock_platform_system("Windows") marker."""
    marker = request.node.get_closest_marker("mock_platform_system")
    if marker:
        target_platform = marker.args[0]
        monkeypatch.setattr("platform.system", lambda: target_platform)
        monkeypatch.setattr(
            "kagan.core.command_utils.is_windows", lambda: target_platform == "Windows"
        )


@pytest.fixture(autouse=True)
def _clean_worktree_base() -> Generator[None, None, None]:
    """Ensure worktree temp directories don't leak between tests."""
    from kagan.core.paths import get_worktree_base_dir

    yield
    shutil.rmtree(get_worktree_base_dir(), ignore_errors=True)
