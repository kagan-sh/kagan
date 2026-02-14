"""GitHub issue sync, mapping, and mode resolution logic."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final

from kagan.core.models.enums import TaskStatus, TaskType

if TYPE_CHECKING:
    from kagan.core.plugins.github.gh_adapter import GhIssue

log = logging.getLogger(__name__)

# Storage keys in Repo.scripts
GITHUB_SYNC_CHECKPOINT_KEY: Final = "kagan.github.sync_checkpoint"
GITHUB_ISSUE_MAPPING_KEY: Final = "kagan.github.issue_mapping"
GITHUB_DEFAULT_MODE_KEY: Final = "kagan.github.default_mode"

# Mode labels for deterministic task type resolution
MODE_LABEL_AUTO: Final = "kagan:mode:auto"
MODE_LABEL_PAIR: Final = "kagan:mode:pair"

# V1 default fallback when no labels and no repo default configured
V1_DEFAULT_MODE: Final = TaskType.PAIR


@dataclass
class SyncCheckpoint:
    """Checkpoint for incremental sync tracking."""

    last_sync_at: str | None = None
    issue_count: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {"last_sync_at": self.last_sync_at, "issue_count": self.issue_count}

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> SyncCheckpoint:
        if not data:
            return cls()
        return cls(
            last_sync_at=data.get("last_sync_at"),
            issue_count=data.get("issue_count", 0),
        )


@dataclass
class IssueMapping:
    """Bidirectional mapping between GitHub issues and Kagan tasks."""

    issue_to_task: dict[int, str] = field(default_factory=dict)
    task_to_issue: dict[str, int] = field(default_factory=dict)

    def get_task_id(self, issue_number: int) -> str | None:
        return self.issue_to_task.get(issue_number)

    def get_issue_number(self, task_id: str) -> int | None:
        return self.task_to_issue.get(task_id)

    def add_mapping(self, issue_number: int, task_id: str) -> None:
        self.issue_to_task[issue_number] = task_id
        self.task_to_issue[task_id] = issue_number

    def remove_by_issue(self, issue_number: int) -> None:
        task_id = self.issue_to_task.pop(issue_number, None)
        if task_id:
            self.task_to_issue.pop(task_id, None)

    def remove_by_task(self, task_id: str) -> None:
        issue_number = self.task_to_issue.pop(task_id, None)
        if issue_number is not None:
            self.issue_to_task.pop(issue_number, None)

    def to_dict(self) -> dict[str, Any]:
        return {
            "issue_to_task": {str(k): v for k, v in self.issue_to_task.items()},
            "task_to_issue": self.task_to_issue,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> IssueMapping:
        if not data:
            return cls()
        issue_to_task = {int(k): v for k, v in data.get("issue_to_task", {}).items()}
        task_to_issue = data.get("task_to_issue", {})
        return cls(issue_to_task=issue_to_task, task_to_issue=task_to_issue)


@dataclass(frozen=True, slots=True)
class ModeResolution:
    """Result of task type resolution from labels.

    Attributes:
        task_type: The resolved TaskType.
        source: Where the mode came from: "label", "repo_default", or "v1_default".
        conflict: True if conflicting labels were present.
    """

    task_type: TaskType
    source: str
    conflict: bool = False


def resolve_task_type_from_labels(
    labels: list[str],
    repo_default: TaskType | None = None,
) -> ModeResolution:
    """Resolve TaskType from issue labels with deterministic precedence.

    Resolution order:
    1. Issue labels (if present)
    2. Repo default (if configured)
    3. V1 default (PAIR)

    Conflict handling: If both kagan:mode:auto and kagan:mode:pair labels
    are present, PAIR wins deterministically and a warning is logged.

    Args:
        labels: List of issue label names.
        repo_default: Optional repo-configured default mode.

    Returns:
        ModeResolution with resolved type, source, and conflict flag.
    """
    labels_lower = {label.lower() for label in labels}
    has_auto = MODE_LABEL_AUTO.lower() in labels_lower
    has_pair = MODE_LABEL_PAIR.lower() in labels_lower

    # Check for conflicting labels
    if has_auto and has_pair:
        log.warning(
            "Conflicting mode labels detected: both %s and %s present. "
            "Resolving to PAIR (deterministic conflict resolution).",
            MODE_LABEL_AUTO,
            MODE_LABEL_PAIR,
        )
        return ModeResolution(task_type=TaskType.PAIR, source="label", conflict=True)

    # Single label present
    if has_pair:
        return ModeResolution(task_type=TaskType.PAIR, source="label")
    if has_auto:
        return ModeResolution(task_type=TaskType.AUTO, source="label")

    # No mode labels - fall back to repo default or V1 default
    if repo_default is not None:
        return ModeResolution(task_type=repo_default, source="repo_default")

    return ModeResolution(task_type=V1_DEFAULT_MODE, source="v1_default")


def build_task_title_from_issue(issue_number: int, issue_title: str) -> str:
    """Build task title with GitHub issue attribution."""
    return f"[GH-{issue_number}] {issue_title}"


def resolve_task_status_from_issue_state(issue_state: str) -> TaskStatus:
    """Map GitHub issue state to Kagan task status.

    OPEN -> BACKLOG (task is actionable)
    CLOSED -> DONE (work is complete)
    """
    if issue_state.upper() == "CLOSED":
        return TaskStatus.DONE
    return TaskStatus.BACKLOG


@dataclass
class SyncOutcome:
    """Result of syncing a single issue to a task projection."""

    issue_number: int
    action: str  # "insert", "update", "reopen", "close", "no_change"
    task_id: str | None = None
    error: str | None = None


@dataclass
class SyncResult:
    """Aggregate result of a full sync operation."""

    success: bool
    outcomes: list[SyncOutcome] = field(default_factory=list)
    inserted: int = 0
    updated: int = 0
    reopened: int = 0
    closed: int = 0
    no_change: int = 0
    errors: int = 0
    error_message: str | None = None

    def add_outcome(self, outcome: SyncOutcome) -> None:
        self.outcomes.append(outcome)
        if outcome.error:
            self.errors += 1
        elif outcome.action == "insert":
            self.inserted += 1
        elif outcome.action == "update":
            self.updated += 1
        elif outcome.action == "reopen":
            self.reopened += 1
        elif outcome.action == "close":
            self.closed += 1
        elif outcome.action == "no_change":
            self.no_change += 1


def load_checkpoint(scripts: dict[str, str] | None) -> SyncCheckpoint:
    """Load sync checkpoint from Repo.scripts."""
    if not scripts:
        return SyncCheckpoint()
    raw = scripts.get(GITHUB_SYNC_CHECKPOINT_KEY)
    if not raw:
        return SyncCheckpoint()
    try:
        data = json.loads(raw) if isinstance(raw, str) else raw
        return SyncCheckpoint.from_dict(data)
    except (json.JSONDecodeError, TypeError):
        return SyncCheckpoint()


def load_mapping(scripts: dict[str, str] | None) -> IssueMapping:
    """Load issue mapping from Repo.scripts."""
    if not scripts:
        return IssueMapping()
    raw = scripts.get(GITHUB_ISSUE_MAPPING_KEY)
    if not raw:
        return IssueMapping()
    try:
        data = json.loads(raw) if isinstance(raw, str) else raw
        return IssueMapping.from_dict(data)
    except (json.JSONDecodeError, TypeError):
        return IssueMapping()


def load_repo_default_mode(scripts: dict[str, str] | None) -> TaskType | None:
    """Load repo default mode from Repo.scripts.

    Args:
        scripts: The repo scripts dict (Repo.scripts).

    Returns:
        TaskType if configured, None otherwise.
    """
    if not scripts:
        return None
    raw = scripts.get(GITHUB_DEFAULT_MODE_KEY)
    if not raw:
        return None
    try:
        value = raw.strip().upper() if isinstance(raw, str) else str(raw).strip().upper()
        if value == TaskType.AUTO.value:
            return TaskType.AUTO
        if value == TaskType.PAIR.value:
            return TaskType.PAIR
        return None
    except (AttributeError, TypeError):
        return None


def compute_issue_changes(
    issue: GhIssue,
    mapping: IssueMapping,
    existing_tasks: dict[str, Any],
    repo_default: TaskType | None = None,
) -> tuple[str, dict[str, Any] | None]:
    """Compute what action is needed for an issue and what fields to change.

    Args:
        issue: The GitHub issue to sync.
        mapping: Current issue-to-task mapping.
        existing_tasks: Dict of task_id -> task data for tasks in mapping.
        repo_default: Optional repo-configured default mode.

    Returns:
        Tuple of (action, changes_dict or None).
        action: "insert", "update", "reopen", "close", or "no_change"
        changes_dict: Fields to set for update/reopen/close, or full data for insert.
    """
    task_id = mapping.get_task_id(issue.number)
    target_status = resolve_task_status_from_issue_state(issue.state)
    target_title = build_task_title_from_issue(issue.number, issue.title)
    mode_resolution = resolve_task_type_from_labels(issue.labels, repo_default)
    target_type = mode_resolution.task_type

    if task_id is None:
        # No existing mapping - need to insert
        return "insert", {
            "title": target_title,
            "description": f"Synced from GitHub issue #{issue.number}",
            "status": target_status,
            "task_type": target_type,
        }

    task = existing_tasks.get(task_id)
    if task is None:
        # Mapping drift: task was deleted, need to recreate
        return "insert", {
            "title": target_title,
            "description": f"Synced from GitHub issue #{issue.number}",
            "status": target_status,
            "task_type": target_type,
        }

    # Task exists - check for changes
    current_title = task.get("title", "")
    current_status = task.get("status", TaskStatus.BACKLOG)
    current_type = task.get("task_type", TaskType.PAIR)

    changes: dict[str, Any] = {}

    # Check title change
    if current_title != target_title:
        changes["title"] = target_title

    # Check task type change
    if current_type != target_type:
        changes["task_type"] = target_type

    # Check status change (reopen/close)
    if current_status != target_status:
        changes["status"] = target_status

    if not changes:
        return "no_change", None

    # Determine action based on status change
    if "status" in changes:
        if changes["status"] == TaskStatus.DONE:
            return "close", changes
        if changes["status"] == TaskStatus.BACKLOG and current_status == TaskStatus.DONE:
            return "reopen", changes

    return "update", changes


__all__ = [
    "GITHUB_DEFAULT_MODE_KEY",
    "GITHUB_ISSUE_MAPPING_KEY",
    "GITHUB_SYNC_CHECKPOINT_KEY",
    "MODE_LABEL_AUTO",
    "MODE_LABEL_PAIR",
    "V1_DEFAULT_MODE",
    "IssueMapping",
    "ModeResolution",
    "SyncCheckpoint",
    "SyncOutcome",
    "SyncResult",
    "build_task_title_from_issue",
    "compute_issue_changes",
    "load_checkpoint",
    "load_mapping",
    "load_repo_default_mode",
    "resolve_task_status_from_issue_state",
    "resolve_task_type_from_labels",
]
