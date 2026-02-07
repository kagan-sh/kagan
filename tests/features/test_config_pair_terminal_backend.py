from __future__ import annotations

import platform

import pytest

from kagan.config import KaganConfig


def test_default_pair_terminal_backend_is_tmux() -> None:
    config = KaganConfig()
    expected = "vscode" if platform.system() == "Windows" else "tmux"
    assert config.general.default_pair_terminal_backend == expected


@pytest.mark.asyncio
async def test_pair_terminal_backend_persists_across_save_load(tmp_path) -> None:
    config_path = tmp_path / "config.toml"
    config = KaganConfig()
    config.general.default_pair_terminal_backend = "cursor"  # type: ignore[assignment]

    await config.save(config_path)
    loaded = KaganConfig.load(config_path)

    assert loaded.general.default_pair_terminal_backend == "cursor"


def test_invalid_pair_terminal_backend_falls_back_to_tmux(tmp_path) -> None:
    config_path = tmp_path / "config.toml"
    config_path.write_text(
        "\n".join(
            [
                "[general]",
                'default_pair_terminal_backend = "invalid-launcher"',
            ]
        ),
        encoding="utf-8",
    )

    loaded = KaganConfig.load(config_path)

    expected = "vscode" if platform.system() == "Windows" else "tmux"
    assert loaded.general.default_pair_terminal_backend == expected
