"""Session binding and authorization lane helpers for CoreHost."""

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING

from kagan.core.security import DEFAULT_PROFILE, AuthorizationPolicy, CapabilityProfile

if TYPE_CHECKING:
    from kagan.core.ipc.contracts import CoreRequest

_PROFILE_RANK: dict[CapabilityProfile, int] = {
    CapabilityProfile.VIEWER: 0,
    CapabilityProfile.PLANNER: 1,
    CapabilityProfile.PAIR_WORKER: 2,
    CapabilityProfile.OPERATOR: 3,
    CapabilityProfile.MAINTAINER: 4,
}


class SessionOrigin(StrEnum):
    LEGACY = "legacy"
    KAGAN = "kagan"
    KAGAN_ADMIN = "kagan_admin"


class SessionNamespace(StrEnum):
    DEFAULT = "default"
    TASK = "task"
    PLANNER = "planner"
    EXT = "ext"


_SCOPED_SESSION_NAMESPACES: frozenset[SessionNamespace] = frozenset(
    {SessionNamespace.TASK, SessionNamespace.PLANNER, SessionNamespace.EXT}
)


_ORIGIN_PROFILE_CEILING: dict[SessionOrigin, CapabilityProfile] = {
    SessionOrigin.LEGACY: CapabilityProfile.MAINTAINER,
    SessionOrigin.KAGAN: CapabilityProfile.PAIR_WORKER,
    SessionOrigin.KAGAN_ADMIN: CapabilityProfile.MAINTAINER,
}

_ORIGIN_ALLOWED_NAMESPACES: dict[SessionOrigin, set[SessionNamespace]] = {
    SessionOrigin.LEGACY: {
        SessionNamespace.DEFAULT,
        SessionNamespace.TASK,
        SessionNamespace.PLANNER,
        SessionNamespace.EXT,
    },
    SessionOrigin.KAGAN: {
        SessionNamespace.DEFAULT,
        SessionNamespace.TASK,
        SessionNamespace.PLANNER,
    },
    SessionOrigin.KAGAN_ADMIN: {SessionNamespace.EXT},
}

_TASK_MUTATION_METHODS: set[tuple[str, str]] = {
    ("jobs", "submit"),
    ("jobs", "get"),
    ("jobs", "wait"),
    ("jobs", "events"),
    ("jobs", "cancel"),
    ("tasks", "update_scratchpad"),
    ("tasks", "delete"),
    ("review", "request"),
}

_LEGACY_TASK_ID_RE = re.compile(r"^[A-Z]+-\d+$")


class SessionBindingError(Exception):
    """Raised when a request violates session binding / lane constraints."""

    def __init__(self, code: str, message: str) -> None:
        self.code = code
        super().__init__(message)


@dataclass(frozen=True)
class SessionBinding:
    """Resolved auth context bound to an IPC session."""

    policy: AuthorizationPolicy
    origin: SessionOrigin
    namespace: SessionNamespace
    scope_id: str


def register_session(
    bindings: dict[str, SessionBinding],
    session_id: str,
    profile: str,
) -> None:
    normalized_profile = _normalize_profile(profile)
    namespace, scope_id = _parse_session_scope(session_id)
    bindings[session_id] = SessionBinding(
        policy=AuthorizationPolicy(str(normalized_profile)),
        origin=SessionOrigin.LEGACY,
        namespace=namespace,
        scope_id=scope_id,
    )


def unregister_session(bindings: dict[str, SessionBinding], session_id: str) -> None:
    bindings.pop(session_id, None)


def get_binding(bindings: dict[str, SessionBinding], request: CoreRequest) -> SessionBinding:
    """Resolve and cache authorization/session lane binding for a request."""
    existing = bindings.get(request.session_id)
    if existing is not None:
        if request.session_profile:
            try:
                requested_profile = _normalize_profile(request.session_profile)
            except ValueError as exc:
                raise SessionBindingError("INVALID_PROFILE", str(exc)) from exc
            try:
                existing_profile = _normalize_profile(existing.policy.profile)
            except ValueError as exc:
                raise SessionBindingError("INVALID_PROFILE", str(exc)) from exc
            if requested_profile is not existing_profile:
                msg = (
                    f"Session '{request.session_id}' is already bound to profile "
                    f"'{existing.policy.profile}', cannot switch to '{request.session_profile}'"
                )
                raise SessionBindingError("INVALID_PROFILE", msg)
        if request.session_origin:
            requested_origin = _normalize_origin(request.session_origin)
            if requested_origin != existing.origin:
                msg = (
                    f"Session '{request.session_id}' is already bound to origin "
                    f"'{existing.origin}', cannot switch to '{requested_origin}'"
                )
                raise SessionBindingError("SESSION_ORIGIN_MISMATCH", msg)
        return existing

    origin = _normalize_origin(request.session_origin)
    try:
        requested_profile = _normalize_profile(str(request.session_profile or DEFAULT_PROFILE))
    except ValueError as exc:
        raise SessionBindingError("INVALID_PROFILE", str(exc)) from exc
    ceiling_profile = _ORIGIN_PROFILE_CEILING[origin]
    effective_profile = _effective_profile(requested_profile, ceiling_profile=ceiling_profile)

    namespace, scope_id = _parse_session_scope(request.session_id)
    allowed_namespaces = _ORIGIN_ALLOWED_NAMESPACES[origin]
    if namespace not in allowed_namespaces:
        allowed = ", ".join(sorted(item.value for item in allowed_namespaces))
        msg = (
            f"Origin '{origin}' is not authorized for session namespace '{namespace}'. "
            f"Allowed namespaces: {allowed}"
        )
        raise SessionBindingError("SESSION_NAMESPACE_DENIED", msg)

    binding = SessionBinding(
        policy=AuthorizationPolicy(str(effective_profile)),
        origin=origin,
        namespace=namespace,
        scope_id=scope_id,
    )
    bindings[request.session_id] = binding
    return binding


def enforce_task_scope(request: CoreRequest, binding: SessionBinding) -> None:
    if (request.capability, request.method) not in _TASK_MUTATION_METHODS:
        return
    if binding.namespace is not SessionNamespace.TASK:
        return
    task_id = request.params.get("task_id")
    if not isinstance(task_id, str) or not task_id.strip():
        msg = f"Task-scoped session '{request.session_id}' requires a non-empty task_id parameter"
        raise SessionBindingError("INVALID_PARAMS", msg)
    if task_id != binding.scope_id:
        msg = (
            f"Session '{request.session_id}' is scoped to task '{binding.scope_id}' "
            f"and cannot mutate task '{task_id}'"
        )
        raise SessionBindingError("SESSION_SCOPE_DENIED", msg)


def _normalize_profile(profile: str) -> CapabilityProfile:
    try:
        return CapabilityProfile(profile)
    except ValueError as exc:
        valid = ", ".join(str(item) for item in CapabilityProfile)
        msg = f"Unknown capability profile '{profile}'. Valid profiles: {valid}"
        raise ValueError(msg) from exc


def _normalize_origin(origin: str | None) -> SessionOrigin:
    if origin is None:
        return SessionOrigin.LEGACY
    normalized = origin.strip().lower()
    if not normalized:
        return SessionOrigin.LEGACY
    try:
        return SessionOrigin(normalized)
    except ValueError as exc:
        valid = ", ".join(sorted(item.value for item in SessionOrigin))
        msg = f"Unknown session origin '{origin}'. Valid origins: {valid}"
        raise SessionBindingError("INVALID_ORIGIN", msg) from exc


def _coerce_namespace(namespace: str) -> SessionNamespace | None:
    try:
        return SessionNamespace(namespace)
    except ValueError:
        return None


def _parse_session_scope(session_id: str) -> tuple[SessionNamespace, str]:
    if ":" in session_id:
        namespace_raw, _, scope = session_id.partition(":")
        namespace = _coerce_namespace(namespace_raw)
        if namespace is not None and namespace in _SCOPED_SESSION_NAMESPACES and scope:
            return namespace, scope
    if _LEGACY_TASK_ID_RE.fullmatch(session_id):
        return SessionNamespace.TASK, session_id
    return SessionNamespace.DEFAULT, session_id


def _effective_profile(
    requested_profile: CapabilityProfile,
    *,
    ceiling_profile: CapabilityProfile,
) -> CapabilityProfile:
    if _PROFILE_RANK[requested_profile] <= _PROFILE_RANK[ceiling_profile]:
        return requested_profile
    return ceiling_profile
