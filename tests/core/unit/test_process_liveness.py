from __future__ import annotations

import os

import kagan.core.process_liveness as process_liveness


def test_pid_exists_non_positive_returns_false() -> None:
    assert process_liveness.pid_exists(0) is False
    assert process_liveness.pid_exists(-1) is False


def test_pid_exists_current_process_is_true() -> None:
    assert process_liveness.pid_exists(os.getpid()) is True


def test_pid_exists_windows_fallback(monkeypatch) -> None:
    calls: list[int] = []
    with monkeypatch.context() as m:
        m.setattr(process_liveness, "_pid_exists_psutil", lambda _pid: None)
        m.setattr(process_liveness.os, "name", "nt", raising=False)

        def _fake_windows(pid: int) -> bool:
            calls.append(pid)
            return True

        m.setattr(process_liveness, "_pid_exists_windows", _fake_windows)
        assert process_liveness.pid_exists(1234) is True
    assert calls == [1234]


def test_pid_exists_posix_permission_error_means_alive(monkeypatch) -> None:
    with monkeypatch.context() as m:
        m.setattr(process_liveness, "_pid_exists_psutil", lambda _pid: None)
        m.setattr(process_liveness.os, "name", "posix", raising=False)

        def _raise_permission_error(_pid: int, _sig: int) -> None:
            raise PermissionError

        m.setattr(process_liveness.os, "kill", _raise_permission_error)
        assert process_liveness.pid_exists(4321) is True


def test_pid_exists_posix_os_error_means_dead(monkeypatch) -> None:
    with monkeypatch.context() as m:
        m.setattr(process_liveness, "_pid_exists_psutil", lambda _pid: None)
        m.setattr(process_liveness.os, "name", "posix", raising=False)

        def _raise_os_error(_pid: int, _sig: int) -> None:
            raise OSError

        m.setattr(process_liveness.os, "kill", _raise_os_error)
        assert process_liveness.pid_exists(4321) is False
