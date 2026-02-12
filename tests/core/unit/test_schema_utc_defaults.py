from __future__ import annotations

from datetime import UTC

from kagan.core.adapters.db.schema import AppState, Project, Task


def test_schema_timestamps_default_to_utc() -> None:
    project = Project(name="UTC Project")
    task = Task(project_id=project.id, title="UTC Task")
    state = AppState(key="runtime_context")

    assert project.created_at.tzinfo == UTC
    assert project.updated_at.tzinfo == UTC
    assert task.created_at.tzinfo == UTC
    assert task.updated_at.tzinfo == UTC
    assert state.updated_at.tzinfo == UTC
