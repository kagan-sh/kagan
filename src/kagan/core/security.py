"""Capability profiles, protocol methods, and authorization policy."""

from __future__ import annotations

from enum import StrEnum

# ---------------------------------------------------------------------------
# Capability profiles and protocol enums
# ---------------------------------------------------------------------------


class CapabilityProfile(StrEnum):
    """Named security profiles for MCP session authorization.

    Profiles are hierarchical — each level includes all permissions from
    the levels below it:

        viewer < planner < pair_worker < operator < maintainer
    """

    VIEWER = "viewer"
    PLANNER = "planner"
    PAIR_WORKER = "pair_worker"
    OPERATOR = "operator"
    MAINTAINER = "maintainer"


class ProtocolCapability(StrEnum):
    """Canonical MCP protocol capability names."""

    TASKS = "tasks"
    PROJECTS = "projects"
    AUDIT = "audit"
    PLAN = "plan"
    JOBS = "jobs"
    REVIEW = "review"
    SESSIONS = "sessions"
    DIAGNOSTICS = "diagnostics"
    SETTINGS = "settings"


class TasksMethod(StrEnum):
    """Task capability methods."""

    CONTEXT = "context"
    GET = "get"
    LIST = "list"
    LOGS = "logs"
    SCRATCHPAD = "scratchpad"
    UPDATE_SCRATCHPAD = "update_scratchpad"
    CREATE = "create"
    UPDATE = "update"
    MOVE = "move"
    DELETE = "delete"


class ProjectsMethod(StrEnum):
    """Project capability methods."""

    GET = "get"
    LIST = "list"
    REPOS = "repos"
    CREATE = "create"
    OPEN = "open"


class AuditMethod(StrEnum):
    """Audit capability methods."""

    LIST = "list"


class PlanMethod(StrEnum):
    """Plan capability methods."""

    PROPOSE = "propose"


class JobsMethod(StrEnum):
    """Jobs capability methods."""

    SUBMIT = "submit"
    GET = "get"
    WAIT = "wait"
    EVENTS = "events"
    CANCEL = "cancel"


class ReviewMethod(StrEnum):
    """Review capability methods."""

    REQUEST = "request"
    APPROVE = "approve"
    REJECT = "reject"
    MERGE = "merge"
    REBASE = "rebase"


class SessionsMethod(StrEnum):
    """Session capability methods."""

    CREATE = "create"
    ATTACH = "attach"
    EXISTS = "exists"
    KILL = "kill"


class DiagnosticsMethod(StrEnum):
    """Diagnostics capability methods."""

    INSTRUMENTATION = "instrumentation"


class SettingsMethod(StrEnum):
    """Settings capability methods."""

    GET = "get"
    UPDATE = "update"


type ProtocolMethod = (
    TasksMethod
    | ProjectsMethod
    | AuditMethod
    | PlanMethod
    | JobsMethod
    | ReviewMethod
    | SessionsMethod
    | DiagnosticsMethod
    | SettingsMethod
)

type CapabilityMethod = tuple[str, str]

VALID_CAPABILITY_PROFILES: frozenset[str] = frozenset(
    profile.value for profile in CapabilityProfile
)


def normalize_profile(profile: CapabilityProfile | str) -> CapabilityProfile:
    """Normalize an input profile to :class:`CapabilityProfile`."""
    try:
        return CapabilityProfile(profile)
    except ValueError as exc:
        valid = ", ".join(sorted(VALID_CAPABILITY_PROFILES))
        msg = f"Unknown capability profile '{profile}'. Valid profiles: {valid}"
        raise ValueError(msg) from exc


def coerce_profile(profile: object) -> CapabilityProfile | None:
    """Coerce an arbitrary value to :class:`CapabilityProfile`."""
    if isinstance(profile, CapabilityProfile):
        return profile
    if isinstance(profile, str):
        try:
            return CapabilityProfile(profile)
        except ValueError:
            return None
    return None


def protocol_call(
    capability: ProtocolCapability | str,
    method: ProtocolMethod | str,
) -> CapabilityMethod:
    """Build a canonical ``(capability, method)`` tuple."""
    canonical_capability = (
        capability.value if isinstance(capability, ProtocolCapability) else capability
    )
    canonical_method = method.value if isinstance(method, StrEnum) else method
    return canonical_capability, canonical_method


# Read-only queries available to every profile.
_VIEWER_METHODS: frozenset[CapabilityMethod] = frozenset(
    {
        protocol_call(ProtocolCapability.TASKS, TasksMethod.CONTEXT),
        protocol_call(ProtocolCapability.TASKS, TasksMethod.GET),
        protocol_call(ProtocolCapability.TASKS, TasksMethod.LIST),
        protocol_call(ProtocolCapability.TASKS, TasksMethod.LOGS),
        protocol_call(ProtocolCapability.TASKS, TasksMethod.SCRATCHPAD),
        protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.GET),
        protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.LIST),
        protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.REPOS),
        protocol_call(ProtocolCapability.AUDIT, AuditMethod.LIST),
    }
)

# Planner adds plan proposal capability.
_PLANNER_METHODS: frozenset[CapabilityMethod] = _VIEWER_METHODS | frozenset(
    {
        protocol_call(ProtocolCapability.PLAN, PlanMethod.PROPOSE),
    }
)

# Pair worker adds scratchpad updates, review request, and session management.
_PAIR_WORKER_METHODS: frozenset[CapabilityMethod] = _PLANNER_METHODS | frozenset(
    {
        protocol_call(ProtocolCapability.TASKS, TasksMethod.UPDATE_SCRATCHPAD),
        protocol_call(ProtocolCapability.JOBS, JobsMethod.SUBMIT),
        protocol_call(ProtocolCapability.JOBS, JobsMethod.GET),
        protocol_call(ProtocolCapability.JOBS, JobsMethod.WAIT),
        protocol_call(ProtocolCapability.JOBS, JobsMethod.EVENTS),
        protocol_call(ProtocolCapability.JOBS, JobsMethod.CANCEL),
        protocol_call(ProtocolCapability.REVIEW, ReviewMethod.REQUEST),
        protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.CREATE),
        protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.ATTACH),
        protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.EXISTS),
        protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.KILL),
    }
)

# Operator adds full task CRUD and review approve/reject.
_OPERATOR_METHODS: frozenset[CapabilityMethod] = _PAIR_WORKER_METHODS | frozenset(
    {
        protocol_call(ProtocolCapability.TASKS, TasksMethod.CREATE),
        protocol_call(ProtocolCapability.TASKS, TasksMethod.UPDATE),
        protocol_call(ProtocolCapability.TASKS, TasksMethod.MOVE),
        protocol_call(ProtocolCapability.REVIEW, ReviewMethod.APPROVE),
        protocol_call(ProtocolCapability.REVIEW, ReviewMethod.REJECT),
    }
)

# Maintainer has unrestricted access.
_MAINTAINER_METHODS: frozenset[CapabilityMethod] = _OPERATOR_METHODS | frozenset(
    {
        protocol_call(ProtocolCapability.TASKS, TasksMethod.DELETE),
        protocol_call(ProtocolCapability.REVIEW, ReviewMethod.MERGE),
        protocol_call(ProtocolCapability.REVIEW, ReviewMethod.REBASE),
        protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.CREATE),
        protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.OPEN),
        protocol_call(ProtocolCapability.DIAGNOSTICS, DiagnosticsMethod.INSTRUMENTATION),
        protocol_call(ProtocolCapability.SETTINGS, SettingsMethod.GET),
        protocol_call(ProtocolCapability.SETTINGS, SettingsMethod.UPDATE),
    }
)

CAPABILITY_PROFILES: dict[CapabilityProfile, frozenset[CapabilityMethod]] = {
    CapabilityProfile.VIEWER: _VIEWER_METHODS,
    CapabilityProfile.PLANNER: _PLANNER_METHODS,
    CapabilityProfile.PAIR_WORKER: _PAIR_WORKER_METHODS,
    CapabilityProfile.OPERATOR: _OPERATOR_METHODS,
    CapabilityProfile.MAINTAINER: _MAINTAINER_METHODS,
}


# ---------------------------------------------------------------------------
# Authorization policy
# ---------------------------------------------------------------------------


class AuthorizationError(Exception):
    """Raised when a request is denied by the authorization policy."""

    code: str = "AUTHORIZATION_DENIED"

    def __init__(
        self,
        capability: ProtocolCapability | str,
        method: ProtocolMethod | str,
        profile: CapabilityProfile,
    ) -> None:
        canonical_capability, canonical_method = protocol_call(capability, method)
        profile_value = profile.value
        self.capability = canonical_capability
        self.method = canonical_method
        self.profile = profile_value
        message = (
            f"Profile '{profile_value}' is not authorized for "
            f"{canonical_capability}.{canonical_method}"
        )
        super().__init__(message)


class AuthorizationPolicy:
    """Checks whether a given profile is allowed to invoke a capability method.

    Usage::

        policy = AuthorizationPolicy("viewer")
        policy.check("tasks", "list")  # True
        policy.check("tasks", "delete")  # False
        policy.enforce("tasks", "delete")  # raises AuthorizationError
    """

    def __init__(self, profile: CapabilityProfile | str) -> None:
        normalized_profile = normalize_profile(profile)
        self._profile = normalized_profile
        self._allowed: frozenset[CapabilityMethod] = CAPABILITY_PROFILES[normalized_profile]
        self._unrestricted = normalized_profile is CapabilityProfile.MAINTAINER

    @property
    def profile(self) -> CapabilityProfile:
        """The profile name this policy enforces."""
        return self._profile

    @property
    def allowed_methods(self) -> frozenset[CapabilityMethod]:
        """The set of (capability, method) tuples this profile may access."""
        return self._allowed

    def check(self, capability: ProtocolCapability | str, method: ProtocolMethod | str) -> bool:
        """Return *True* if the profile is allowed to call *capability.method*.

        The ``maintainer`` profile is unrestricted and always returns *True*.
        """
        if self._unrestricted:
            return True
        canonical_call = protocol_call(capability, method)
        return canonical_call in self._allowed

    def enforce(self, capability: ProtocolCapability | str, method: ProtocolMethod | str) -> None:
        """Raise :class:`AuthorizationError` if the call is not allowed."""
        if not self.check(capability, method):
            raise AuthorizationError(capability, method, self._profile)


# Default policy for unscoped sessions.
DEFAULT_PROFILE = CapabilityProfile.VIEWER


__all__ = [
    "CAPABILITY_PROFILES",
    "DEFAULT_PROFILE",
    "VALID_CAPABILITY_PROFILES",
    "AuthorizationError",
    "AuthorizationPolicy",
    "CapabilityMethod",
    "CapabilityProfile",
    "ProtocolCapability",
    "ProtocolMethod",
    "coerce_profile",
    "normalize_profile",
    "protocol_call",
]
