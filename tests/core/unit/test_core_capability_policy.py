"""Tests for capability profiles and authorization policy."""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from kagan.core.ipc.contracts import CoreRequest
from kagan.core.security import (
    CAPABILITY_PROFILES,
    AuthorizationError,
    AuthorizationPolicy,
    CapabilityProfile,
)


async def _dispatch_request(host, request):
    return await host.handle_request(request)


def _set_request_handler(
    monkeypatch: pytest.MonkeyPatch,
    capability: str,
    method: str,
    handler,
) -> None:
    monkeypatch.setattr("kagan.core.host._REQUEST_DISPATCH_MAP", {(capability, method): handler})


# ---------------------------------------------------------------------------
# Profile hierarchy and policy invariants
# ---------------------------------------------------------------------------

PROFILE_ORDER = [
    CapabilityProfile.VIEWER,
    CapabilityProfile.PLANNER,
    CapabilityProfile.PAIR_WORKER,
    CapabilityProfile.OPERATOR,
    CapabilityProfile.MAINTAINER,
]

HIGH_IMPACT_MUTATIONS = {
    ("tasks", "delete"),
    ("review", "merge"),
    ("review", "rebase"),
    ("projects", "create"),
    ("projects", "open"),
    ("settings", "update"),
}
SENSITIVE_DIAGNOSTICS = {
    ("diagnostics", "instrumentation"),
}


class TestCapabilityProfiles:
    """Verify role hierarchy and security-critical invariants."""

    def test_profile_mapping_contains_every_profile(self):
        assert set(CAPABILITY_PROFILES) == set(PROFILE_ORDER)

    def test_profiles_are_strictly_hierarchical(self):
        for index in range(len(PROFILE_ORDER) - 1):
            lower = CAPABILITY_PROFILES[PROFILE_ORDER[index]]
            upper = CAPABILITY_PROFILES[PROFILE_ORDER[index + 1]]
            assert lower < upper, (
                f"{PROFILE_ORDER[index]} should be a strict subset of {PROFILE_ORDER[index + 1]}"
            )

    def test_viewer_profile_excludes_high_impact_mutations(self):
        viewer = CAPABILITY_PROFILES[CapabilityProfile.VIEWER]
        assert viewer.isdisjoint(HIGH_IMPACT_MUTATIONS)

    def test_maintainer_profile_includes_high_impact_mutations(self):
        maintainer = CAPABILITY_PROFILES[CapabilityProfile.MAINTAINER]
        assert HIGH_IMPACT_MUTATIONS.issubset(maintainer)

    def test_pair_worker_profile_includes_job_events_query(self):
        pair_worker = CAPABILITY_PROFILES[CapabilityProfile.PAIR_WORKER]
        assert ("jobs", "events") in pair_worker


class TestAuthorizationPolicy:
    @pytest.mark.parametrize("profile", PROFILE_ORDER[:-1])
    def test_non_maintainers_deny_high_impact_mutations(self, profile):
        policy = AuthorizationPolicy(profile)
        for capability, method in HIGH_IMPACT_MUTATIONS:
            assert not policy.check(capability, method)

    @pytest.mark.parametrize("profile", PROFILE_ORDER[:-1])
    def test_non_maintainers_deny_sensitive_diagnostics(self, profile):
        policy = AuthorizationPolicy(profile)
        for capability, method in SENSITIVE_DIAGNOSTICS:
            assert not policy.check(capability, method)

    def test_maintainer_allows_high_impact_mutations(self):
        policy = AuthorizationPolicy(CapabilityProfile.MAINTAINER)
        for capability, method in HIGH_IMPACT_MUTATIONS:
            assert policy.check(capability, method)

    def test_maintainer_allows_sensitive_diagnostics(self):
        policy = AuthorizationPolicy(CapabilityProfile.MAINTAINER)
        for capability, method in SENSITIVE_DIAGNOSTICS:
            assert policy.check(capability, method)

    def test_maintainer_is_unrestricted_for_unregistered_methods(self):
        policy = AuthorizationPolicy(CapabilityProfile.MAINTAINER)
        assert policy.check("custom", "future_method")

    def test_enforce_raises_on_denied(self):
        policy = AuthorizationPolicy(CapabilityProfile.VIEWER)
        with pytest.raises(AuthorizationError) as exc_info:
            policy.enforce("tasks", "delete")
        assert exc_info.value.code == "AUTHORIZATION_DENIED"
        assert "viewer" in str(exc_info.value)
        assert "tasks.delete" in str(exc_info.value)

    def test_enforce_passes_on_allowed(self):
        policy = AuthorizationPolicy(CapabilityProfile.VIEWER)
        policy.enforce("tasks", "list")

    def test_viewer_denied_job_events_query(self):
        policy = AuthorizationPolicy(CapabilityProfile.VIEWER)
        assert not policy.check("jobs", "events")

    def test_unknown_profile_raises_value_error(self):
        with pytest.raises(ValueError, match="Unknown capability profile"):
            AuthorizationPolicy("superadmin")

    def test_allowed_methods_returns_immutable_set(self):
        policy = AuthorizationPolicy(CapabilityProfile.VIEWER)
        methods = policy.allowed_methods
        assert isinstance(methods, frozenset)
        with pytest.raises(AttributeError):
            methods.add(("tasks", "delete"))  # type: ignore[attr-defined]


# ---------------------------------------------------------------------------
# CoreHost authorization integration tests
# ---------------------------------------------------------------------------


class TestCoreHostAuthorization:
    """Test authorization enforcement in CoreHost.handle_request."""

    @pytest.fixture()
    def host(self):
        from kagan.core.host import CoreHost

        return CoreHost()

    @pytest.mark.asyncio()
    async def test_unregistered_session_defaults_to_viewer(self, host, monkeypatch):
        """Sessions without a registered profile get viewer access."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"ok": True}

        _set_request_handler(monkeypatch, "tasks", "delete", mock_handler)
        host._ctx = SimpleNamespace(api=object())

        request = CoreRequest(
            session_id="unknown-session",
            capability="tasks",
            method="delete",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "AUTHORIZATION_DENIED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_viewer_session_blocked_from_mutation(self, host):
        """A viewer session cannot call mutating commands."""
        host._ctx = SimpleNamespace(api=object())
        host.register_session("viewer-session", "viewer")

        request = CoreRequest(
            session_id="viewer-session",
            capability="tasks",
            method="delete",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "AUTHORIZATION_DENIED"

    @pytest.mark.asyncio()
    async def test_viewer_session_allowed_read_query(self, host, monkeypatch):
        """A viewer session can call read-only queries."""
        call_log = []

        async def mock_query(api, params):
            del api, params
            call_log.append(True)
            return {"tasks": []}

        _set_request_handler(monkeypatch, "tasks", "list", mock_query)
        host._ctx = SimpleNamespace(api=object())
        host.register_session("viewer-session", "viewer")

        request = CoreRequest(
            session_id="viewer-session",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert response.ok
        assert len(call_log) == 1

    @pytest.mark.asyncio()
    async def test_operator_session_blocked_from_merge(self, host):
        """An operator session cannot merge reviews."""
        host._ctx = SimpleNamespace(api=object())
        host.register_session("op-session", "operator")

        request = CoreRequest(
            session_id="op-session",
            capability="review",
            method="merge",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "AUTHORIZATION_DENIED"

    @pytest.mark.asyncio()
    async def test_unregister_session_reverts_to_default(self, host, monkeypatch):
        """After unregister, the session falls back to viewer."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"deleted": True}

        _set_request_handler(monkeypatch, "tasks", "delete", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host.register_session("temp-session", "viewer")

        # Viewer cannot delete
        request = CoreRequest(
            session_id="temp-session",
            capability="tasks",
            method="delete",
        )
        response = await _dispatch_request(host, request)
        assert not response.ok

        # Unregister reverts to viewer
        host.unregister_session("temp-session")
        response = await _dispatch_request(host, request)
        assert not response.ok
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_request_profile_binds_session(self, host, monkeypatch):
        """A request can bind a profile for an otherwise unknown session."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"created": True}

        _set_request_handler(monkeypatch, "tasks", "create", mock_handler)
        host._ctx = SimpleNamespace(api=object())

        request = CoreRequest(
            session_id="mcp-session",
            session_profile="operator",
            capability="tasks",
            method="create",
        )
        response = await _dispatch_request(host, request)

        assert response.ok
        assert len(call_log) == 1

    @pytest.mark.asyncio()
    async def test_request_profile_cannot_change_after_binding(self, host):
        """Once bound, a session profile cannot be changed."""
        host._ctx = SimpleNamespace(api=object())
        host.register_session("mcp-session", "operator")

        request = CoreRequest(
            session_id="mcp-session",
            session_profile="viewer",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "INVALID_PROFILE"

    @pytest.mark.asyncio()
    async def test_kagan_origin_caps_requested_profile(self, host, monkeypatch):
        """The restricted 'kagan' lane cannot escalate beyond pair_worker."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"created": True}

        _set_request_handler(monkeypatch, "tasks", "create", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-401",
            session_profile="maintainer",
            session_origin="kagan",
            client_version="test-version",
            capability="tasks",
            method="create",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "AUTHORIZATION_DENIED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_kagan_admin_origin_requires_ext_namespace(self, host):
        """The admin lane must use ext:* session namespace."""
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-402",
            session_profile="maintainer",
            session_origin="kagan_admin",
            client_version="test-version",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "SESSION_NAMESPACE_DENIED"

    @pytest.mark.asyncio()
    async def test_task_scoped_session_cannot_mutate_other_task(self, host, monkeypatch):
        """Task lane mutations are constrained to the scoped task ID."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"success": True}

        _set_request_handler(monkeypatch, "tasks", "update_scratchpad", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-403",
            session_profile="pair_worker",
            session_origin="kagan",
            client_version="test-version",
            capability="tasks",
            method="update_scratchpad",
            params={"task_id": "TASK-404", "content": "notes"},
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "SESSION_SCOPE_DENIED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_task_scoped_session_cannot_submit_job_for_other_task(self, host, monkeypatch):
        """Task-scoped sessions cannot submit jobs for a different task ID."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"success": True}

        _set_request_handler(monkeypatch, "jobs", "submit", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-500",
            session_profile="pair_worker",
            session_origin="kagan",
            client_version="test-version",
            capability="jobs",
            method="submit",
            params={"task_id": "TASK-501", "action": "start_agent"},
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "SESSION_SCOPE_DENIED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_task_scoped_session_cannot_wait_job_for_other_task(self, host, monkeypatch):
        """Task-scoped sessions cannot query job wait for a different task ID."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"success": True}

        _set_request_handler(monkeypatch, "jobs", "wait", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-600",
            session_profile="pair_worker",
            session_origin="kagan",
            client_version="test-version",
            capability="jobs",
            method="wait",
            params={"job_id": "job-1", "task_id": "TASK-601"},
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "SESSION_SCOPE_DENIED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_task_scoped_session_cannot_query_job_events_for_other_task(
        self,
        host,
        monkeypatch,
    ):
        """Task-scoped sessions cannot query job events for a different task ID."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"success": True}

        _set_request_handler(monkeypatch, "jobs", "events", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-700",
            session_profile="pair_worker",
            session_origin="kagan",
            client_version="test-version",
            capability="jobs",
            method="events",
            params={"job_id": "job-1", "task_id": "TASK-701"},
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "SESSION_SCOPE_DENIED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_kagan_admin_ext_namespace_allows_maintainer_ops(self, host, monkeypatch):
        """Explicit admin lane can execute maintainer-only commands."""
        call_log = []

        async def mock_handler(api, params):
            del api
            call_log.append(params["task_id"])
            return {"success": True}

        _set_request_handler(monkeypatch, "tasks", "delete", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="ext:orchestrator",
            session_profile="maintainer",
            session_origin="kagan_admin",
            client_version="test-version",
            capability="tasks",
            method="delete",
            params={"task_id": "TASK-405"},
        )
        response = await _dispatch_request(host, request)

        assert response.ok
        assert call_log == ["TASK-405"]

    @pytest.mark.asyncio()
    async def test_tui_origin_requires_tui_namespace(self, host):
        """TUI lane is restricted to tui:* session namespace."""
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="default-session",
            session_profile="maintainer",
            session_origin="tui",
            client_version="test-version",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "SESSION_NAMESPACE_DENIED"

    @pytest.mark.asyncio()
    async def test_tui_origin_tui_namespace_allows_maintainer_ops(self, host, monkeypatch):
        """TUI lane can run maintainer operations from tui:* sessions."""
        call_log = []

        async def mock_handler(api, params):
            del api
            call_log.append(params["task_id"])
            return {"success": True}

        _set_request_handler(monkeypatch, "tasks", "delete", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="tui:instance-1",
            session_profile="maintainer",
            session_origin="tui",
            client_version="test-version",
            capability="tasks",
            method="delete",
            params={"task_id": "TASK-406"},
        )
        response = await _dispatch_request(host, request)

        assert response.ok
        assert call_log == ["TASK-406"]

    @pytest.mark.asyncio()
    async def test_kagan_origin_rejects_missing_client_version(self, host, monkeypatch):
        """Kagan-origin requests must include a client version for compatibility checks."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"ok": True}

        _set_request_handler(monkeypatch, "tasks", "list", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-800",
            session_profile="pair_worker",
            session_origin="kagan",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "MCP_OUTDATED"
        assert len(call_log) == 0

    @pytest.mark.asyncio()
    async def test_kagan_origin_rejects_mismatched_client_version(self, host, monkeypatch):
        """Kagan-origin requests are rejected when MCP/client and core versions differ."""
        call_log = []

        async def mock_handler(api, params):
            del api, params
            call_log.append(True)
            return {"ok": True}

        _set_request_handler(monkeypatch, "tasks", "list", mock_handler)
        host._ctx = SimpleNamespace(api=object())
        host._runtime_version = "test-version"

        request = CoreRequest(
            session_id="task:TASK-801",
            session_profile="pair_worker",
            session_origin="kagan",
            client_version="old-version",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "MCP_OUTDATED"
        assert len(call_log) == 0
