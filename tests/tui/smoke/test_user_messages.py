from __future__ import annotations

from kagan.tui.ui.user_messages import (
    instance_lock_copy,
    permission_timer_line,
    task_deleted_close_message,
    task_moved_close_message,
)


def test_instance_lock_copy_covers_startup_and_switch_hints() -> None:
    startup = instance_lock_copy(is_startup=True)
    switch = instance_lock_copy(is_startup=False)

    assert startup.button_label == "Quit"
    assert "start Kagan again" in startup.note
    assert switch.button_label == "OK"
    assert "continue in your current repository" in switch.note


def test_permission_timer_line_formats_countdown() -> None:
    assert permission_timer_line(125) == "Waiting for decision... (2:05)"
    assert permission_timer_line(-3) == "Waiting for decision... (0:00)"


def test_task_close_messages_are_explicit() -> None:
    assert task_deleted_close_message("review") == (
        "Task was deleted by another action. Closing review."
    )
    assert task_moved_close_message("done") == "Task moved to DONE. Closing task output."
