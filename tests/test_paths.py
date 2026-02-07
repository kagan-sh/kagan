"""Tests for XDG path helpers."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.paths import (
    ensure_directories,
    get_cache_dir,
    get_config_dir,
    get_config_path,
    get_data_dir,
    get_database_path,
    get_debug_log_path,
    get_profiles_path,
    get_worktree_base_dir,
)

if TYPE_CHECKING:
    from pathlib import Path


def test_path_overrides(monkeypatch, tmp_path: Path) -> None:
    data_dir = tmp_path / "data"
    config_dir = tmp_path / "config"
    cache_dir = tmp_path / "cache"
    worktree_dir = tmp_path / "worktrees"

    monkeypatch.setenv("KAGAN_DATA_DIR", str(data_dir))
    monkeypatch.setenv("KAGAN_CONFIG_DIR", str(config_dir))
    monkeypatch.setenv("KAGAN_CACHE_DIR", str(cache_dir))
    monkeypatch.setenv("KAGAN_WORKTREE_BASE", str(worktree_dir))

    assert get_data_dir() == data_dir
    assert get_config_dir() == config_dir
    assert get_cache_dir() == cache_dir
    assert get_worktree_base_dir() == worktree_dir


def test_derived_paths_from_data_and_config(monkeypatch, tmp_path: Path) -> None:
    data_dir = tmp_path / "data"
    config_dir = tmp_path / "config"

    monkeypatch.setenv("KAGAN_DATA_DIR", str(data_dir))
    monkeypatch.setenv("KAGAN_CONFIG_DIR", str(config_dir))

    assert get_database_path() == data_dir / "kagan.db"
    assert get_debug_log_path() == data_dir / "debug.log"
    assert get_config_path() == config_dir / "config.toml"
    assert get_profiles_path() == config_dir / "profiles.toml"


def test_ensure_directories_creates(monkeypatch, tmp_path: Path) -> None:
    data_dir = tmp_path / "data"
    config_dir = tmp_path / "config"
    cache_dir = tmp_path / "cache"
    worktree_dir = tmp_path / "worktrees"

    monkeypatch.setenv("KAGAN_DATA_DIR", str(data_dir))
    monkeypatch.setenv("KAGAN_CONFIG_DIR", str(config_dir))
    monkeypatch.setenv("KAGAN_CACHE_DIR", str(cache_dir))
    monkeypatch.setenv("KAGAN_WORKTREE_BASE", str(worktree_dir))

    ensure_directories()

    assert data_dir.is_dir()
    assert config_dir.is_dir()
    assert cache_dir.is_dir()
    assert worktree_dir.is_dir()
