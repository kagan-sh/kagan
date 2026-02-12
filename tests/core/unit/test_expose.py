"""Unit tests for the @expose metadata decorator."""

from __future__ import annotations

from kagan.core.expose import EXPOSE_ATTR, ExposeMetadata, collect_exposed_methods, expose


class TestExposeDecorator:
    """Tests that @expose attaches correct metadata to functions."""

    def test_expose_attaches_metadata(self) -> None:
        @expose("tasks", "get", description="Get task.")
        async def get_task(task_id: str) -> None: ...

        meta = getattr(get_task, EXPOSE_ATTR, None)
        assert meta is not None
        assert isinstance(meta, ExposeMetadata)

    def test_expose_metadata_fields_are_correct(self) -> None:
        @expose(
            "review",
            "approve",
            profile="operator",
            mutating=True,
            description="Approve a review.",
        )
        async def approve_task(task_id: str) -> None: ...

        meta: ExposeMetadata = getattr(approve_task, EXPOSE_ATTR)
        assert meta.capability == "review"
        assert meta.method == "approve"
        assert meta.profile == "operator"
        assert meta.mutating is True
        assert meta.description == "Approve a review."

    def test_expose_defaults(self) -> None:
        @expose("audit", "list")
        async def list_events() -> None: ...

        meta: ExposeMetadata = getattr(list_events, EXPOSE_ATTR)
        assert meta.profile == "viewer"
        assert meta.mutating is False
        assert meta.description == ""

    def test_undecorated_method_has_no_expose_attr(self) -> None:
        async def plain_method() -> None: ...

        assert not hasattr(plain_method, EXPOSE_ATTR)

    def test_expose_preserves_original_function(self) -> None:
        @expose("tasks", "create", mutating=True)
        async def create_task(title: str) -> str: ...

        assert create_task.__name__ == "create_task"
        assert callable(create_task)

    def test_expose_metadata_is_frozen(self) -> None:
        @expose("tasks", "get")
        async def get_task() -> None: ...

        meta: ExposeMetadata = getattr(get_task, EXPOSE_ATTR)
        try:
            meta.capability = "projects"  # type: ignore[misc]
            raise AssertionError("Expected FrozenInstanceError")
        except AttributeError:
            pass


class TestCollectExposedMethods:
    """Tests for collect_exposed_methods helper."""

    def test_collects_decorated_methods(self) -> None:
        class FakeApi:
            @expose("tasks", "get")
            async def get_task(self, task_id: str) -> None: ...

            @expose("tasks", "list")
            async def list_tasks(self) -> None: ...

            async def plain_method(self) -> None: ...

        api = FakeApi()
        results = collect_exposed_methods(api)
        names = [name for name, _method, _meta in results]
        assert "get_task" in names
        assert "list_tasks" in names
        assert "plain_method" not in names
        assert len(results) == 2

    def test_skips_private_methods(self) -> None:
        class FakeApi:
            @expose("tasks", "internal")
            async def _private(self) -> None: ...

        api = FakeApi()
        results = collect_exposed_methods(api)
        assert len(results) == 0

    def test_empty_object_returns_empty(self) -> None:
        results = collect_exposed_methods(object())
        assert results == []


class TestApiExposeIntegration:
    """Verify that the real KaganAPI has @expose decorators applied."""

    def test_api_has_expected_exposed_count(self) -> None:
        from kagan.core.api import KaganAPI

        # Collect from the class itself (unbound methods still carry the attr).
        exposed = []
        for name in dir(KaganAPI):
            if name.startswith("_"):
                continue
            attr = getattr(KaganAPI, name, None)
            if attr is not None and hasattr(attr, EXPOSE_ATTR):
                exposed.append(name)
        assert len(exposed) == 25

    def test_excluded_methods_are_not_exposed(self) -> None:
        from kagan.core.api import KaganAPI

        excluded = {
            "submit_job",
            "cancel_job",
            "get_job",
            "wait_job",
            "get_job_events",
            "merge_task",
            "rebase_task",
            "create_session",
            "kill_session",
            "attach_session",
            "session_exists",
        }
        for name in excluded:
            attr = getattr(KaganAPI, name, None)
            assert attr is not None, f"Method {name} not found on KaganAPI"
            assert not hasattr(attr, EXPOSE_ATTR), f"Method {name} should NOT have @expose but does"
