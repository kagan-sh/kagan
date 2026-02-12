"""Windows compatibility tests for command lexing, preflight, and transport selection."""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

import pytest

from kagan.core.command_utils import split_command_string
from kagan.core.ipc.transports import DefaultTransport, TCPLoopbackTransport, UnixSocketTransport

if TYPE_CHECKING:
    from pytest import MonkeyPatch


def test_split_command_uses_mslex_on_windows(monkeypatch: MonkeyPatch) -> None:
    """Use mslex parsing rules on Windows when available."""

    class FakeMslex:
        @staticmethod
        def split(command: str) -> list[str]:
            return ["MSLEX", command]

        @staticmethod
        def quote(value: str) -> str:
            return f"<{value}>"

        @staticmethod
        def join(args: list[str]) -> str:
            return "|".join(args)

    monkeypatch.setattr("platform.system", lambda: "Windows")
    monkeypatch.setattr("kagan.core.command_utils.is_windows", lambda: True)
    monkeypatch.setitem(sys.modules, "mslex", FakeMslex)  # type: ignore[bad-argument-type]

    assert split_command_string("opencode --prompt hello") == ["MSLEX", "opencode --prompt hello"]


# ---------------------------------------------------------------------------
# DefaultTransport platform tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(sys.platform == "win32", reason="Only applies on non-Windows")
def test_default_transport_is_unix_on_posix() -> None:
    """DefaultTransport resolves to UnixSocketTransport on macOS/Linux."""
    assert DefaultTransport is UnixSocketTransport


@pytest.mark.skipif(sys.platform != "win32", reason="Only applies on Windows")
def test_default_transport_is_tcp_on_windows() -> None:
    """DefaultTransport resolves to TCPLoopbackTransport on Windows."""
    assert DefaultTransport is TCPLoopbackTransport


def test_tcp_loopback_transport_instantiates() -> None:
    """TCPLoopbackTransport can always be instantiated explicitly."""
    transport = TCPLoopbackTransport()
    assert isinstance(transport, TCPLoopbackTransport)


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
def test_unix_socket_transport_instantiates() -> None:
    """UnixSocketTransport can be instantiated on POSIX platforms."""
    transport = UnixSocketTransport()
    assert isinstance(transport, UnixSocketTransport)
