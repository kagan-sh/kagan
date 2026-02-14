"""GitHub plugin application use cases."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any, Final

from kagan.core.models.enums import TaskStatus
from kagan.core.plugins.github.contract import (
    GITHUB_CANONICAL_METHODS,
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_CONTRACT_VERSION,
    GITHUB_PLUGIN_ID,
    RESERVED_GITHUB_CAPABILITY,
)
from kagan.core.plugins.github.domain.repo_state import (
    encode_connection_update,
    encode_pr_mapping_update,
    encode_sync_state_update,
    load_connection_state,
    load_issue_mapping_state,
    load_lease_enforcement_state,
    load_pr_mapping_state,
    load_repo_default_mode_state,
    load_sync_checkpoint_state,
    resolve_owner_repo,
)
from kagan.core.plugins.github.gh_adapter import (
    ALREADY_CONNECTED,
    GH_PR_CREATE_FAILED,
    GH_PR_NOT_FOUND,
    GH_PROJECT_REQUIRED,
    GH_REPO_METADATA_INVALID,
    GH_REPO_REQUIRED,
)
from kagan.core.plugins.github.lease import LEASE_HELD_BY_OTHER
from kagan.core.plugins.github.sync import (
    IssueMapping,
    SyncCheckpoint,
    SyncOutcome,
    SyncResult,
    compute_issue_changes,
    filter_issues_since_checkpoint,
)
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from kagan.core.plugins.github.domain.models import (
        AcquireLeaseInput,
        ConnectRepoInput,
        ContractProbeInput,
        CreatePrForTaskInput,
        GetLeaseStateInput,
        LinkPrToTaskInput,
        ReconcilePrStatusInput,
        ReleaseLeaseInput,
        SyncIssuesInput,
        ValidateReviewTransitionInput,
    )
    from kagan.core.plugins.github.ports.core_gateway import GitHubCoreGateway
    from kagan.core.plugins.github.ports.gh_client import GitHubClient

GH_NOT_CONNECTED: Final = "GH_NOT_CONNECTED"
GH_SYNC_FAILED: Final = "GH_SYNC_FAILED"
GH_ISSUE_REQUIRED: Final = "GH_ISSUE_REQUIRED"
GH_ISSUE_NUMBER_INVALID: Final = "GH_ISSUE_NUMBER_INVALID"
GH_TASK_REQUIRED: Final = "GH_TASK_REQUIRED"
GH_WORKSPACE_REQUIRED: Final = "GH_WORKSPACE_REQUIRED"
GH_PR_NUMBER_REQUIRED: Final = "GH_PR_NUMBER_REQUIRED"
GH_PR_NUMBER_INVALID: Final = "GH_PR_NUMBER_INVALID"
GH_NO_LINKED_PR: Final = "GH_NO_LINKED_PR"
CONNECTED: Final = "CONNECTED"
SYNCED: Final = "SYNCED"
LEASE_STATE_OK: Final = "LEASE_STATE_OK"
LEASE_STATE_ERROR: Final = "LEASE_STATE_ERROR"
PR_CREATED: Final = "PR_CREATED"
PR_LINKED: Final = "PR_LINKED"
PR_STATUS_RECONCILED: Final = "PR_STATUS_RECONCILED"
LEASE_ENFORCEMENT_DISABLED: Final = "LEASE_ENFORCEMENT_DISABLED"
REVIEW_BLOCKED_NO_PR: Final = "REVIEW_BLOCKED_NO_PR"
REVIEW_BLOCKED_LEASE: Final = "REVIEW_BLOCKED_LEASE"
REVIEW_GUARDRAIL_CHECK_FAILED: Final = "REVIEW_GUARDRAIL_CHECK_FAILED"
_SYNC_TASK_WRITE_ERRORS: Final = (ValueError, RuntimeError, LookupError)


class GitHubPluginUseCases:
    """Use-case orchestrator for official GitHub plugin operations."""

    def __init__(self, core_gateway: GitHubCoreGateway, gh_client: GitHubClient) -> None:
        self._core = core_gateway
        self._gh = gh_client

    @staticmethod
    def build_contract_probe_payload(request: ContractProbeInput) -> dict[str, Any]:
        """Return a stable, machine-readable contract response for probe calls."""
        return {
            "success": True,
            "plugin_id": GITHUB_PLUGIN_ID,
            "contract_version": GITHUB_CONTRACT_VERSION,
            "capability": GITHUB_CAPABILITY,
            "method": GITHUB_CONTRACT_PROBE_METHOD,
            "canonical_methods": list(GITHUB_CANONICAL_METHODS),
            "reserved_official_capability": RESERVED_GITHUB_CAPABILITY,
            "echo": request.echo,
        }

    async def connect_repo(self, request: ConnectRepoInput) -> dict[str, Any]:
        """Connect a repository to GitHub with preflight checks."""
        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        connection_state = load_connection_state(repo.scripts)
        if connection_state.raw_value:
            connection_data = connection_state.normalized
            if connection_data is None:
                return self._error(
                    GH_REPO_METADATA_INVALID,
                    "Stored GitHub connection metadata is invalid",
                    "Reconnect the repository using connect_repo to refresh metadata.",
                )

            if connection_state.needs_rewrite:
                await self._core.update_repo_scripts(
                    repo.id,
                    encode_connection_update(connection_data),
                )

            return {
                "success": True,
                "code": ALREADY_CONNECTED,
                "message": "Repository is already connected to GitHub",
                "connection": connection_data,
            }

        repo_view, error = await asyncio.to_thread(self._gh.run_preflight_checks, repo.path)
        if error is not None:
            return {
                "success": False,
                "code": error.code,
                "message": error.message,
                "hint": error.hint,
            }

        assert repo_view is not None
        connection_metadata = self._gh.build_connection_metadata(repo_view)
        await self._core.update_repo_scripts(repo.id, encode_connection_update(connection_metadata))

        return {
            "success": True,
            "code": CONNECTED,
            "message": f"Connected to {repo_view.full_name}",
            "connection": connection_metadata,
        }

    async def sync_issues(self, request: SyncIssuesInput) -> dict[str, Any]:
        """Sync GitHub issues to Kagan task projections."""
        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        _, connection_error = self._resolve_connected_repo_context(repo)
        if connection_error is not None:
            return connection_error

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        raw_issues, error = await asyncio.to_thread(self._gh.run_gh_issue_list, gh_path, repo.path)
        if error:
            return {
                "success": False,
                "code": GH_SYNC_FAILED,
                "message": f"Failed to fetch issues: {error}",
                "hint": "Check gh CLI authentication and repository access",
            }

        all_issues = self._gh.parse_issue_list(raw_issues or [])
        checkpoint = load_sync_checkpoint_state(repo.scripts)
        issues = filter_issues_since_checkpoint(all_issues, checkpoint)

        mapping = load_issue_mapping_state(repo.scripts)
        repo_default_mode = load_repo_default_mode_state(repo.scripts)
        existing_tasks = await self._load_mapped_tasks(mapping)

        result = SyncResult()
        new_mapping = IssueMapping(
            issue_to_task=dict(mapping.issue_to_task),
            task_to_issue=dict(mapping.task_to_issue),
        )

        for issue in issues:
            action, changes = compute_issue_changes(
                issue,
                mapping,
                existing_tasks,
                repo_default_mode,
            )

            if action == "no_change" or changes is None:
                result.add_outcome(
                    SyncOutcome(
                        issue_number=issue.number,
                        action="no_change",
                        task_id=mapping.get_task_id(issue.number),
                    )
                )
                continue

            if action == "insert":
                try:
                    normalized_project_id = self._non_empty_str(request.project_id)
                    assert normalized_project_id is not None
                    task = await self._core.create_task(
                        title=changes["title"],
                        description=changes["description"],
                        project_id=normalized_project_id,
                    )
                    update_fields: dict[str, Any] = {}
                    if changes.get("task_type"):
                        update_fields["task_type"] = changes["task_type"]
                    if changes.get("status"):
                        update_fields["status"] = changes["status"]
                    if update_fields:
                        await self._core.update_task_fields(task.id, **update_fields)
                    new_mapping.remove_by_issue(issue.number)
                    new_mapping.add_mapping(issue.number, task.id)
                    result.add_outcome(
                        SyncOutcome(issue_number=issue.number, action="insert", task_id=task.id)
                    )
                except _SYNC_TASK_WRITE_ERRORS as exc:
                    result.add_outcome(
                        SyncOutcome(issue_number=issue.number, action="insert", error=str(exc))
                    )
                continue

            task_id = mapping.get_task_id(issue.number)
            if not task_id:
                continue

            try:
                await self._core.update_task_fields(task_id, **changes)
                result.add_outcome(
                    SyncOutcome(issue_number=issue.number, action=action, task_id=task_id)
                )
            except _SYNC_TASK_WRITE_ERRORS as exc:
                result.add_outcome(
                    SyncOutcome(issue_number=issue.number, action=action, error=str(exc))
                )

        stats = {
            "total": len(issues),
            "inserted": result.inserted,
            "updated": result.updated,
            "reopened": result.reopened,
            "closed": result.closed,
            "no_change": result.no_change,
            "errors": result.errors,
        }

        if result.errors > 0:
            return {
                "success": False,
                "code": GH_SYNC_FAILED,
                "message": f"Synced {len(issues)} issues with {result.errors} per-issue errors",
                "hint": (
                    "Review failing issue mappings and retry sync after fixing task create/update "
                    "errors."
                ),
                "stats": stats,
            }

        new_checkpoint = SyncCheckpoint(
            last_sync_at=utc_now().isoformat(),
            issue_count=len(issues),
        )
        await self._core.update_repo_scripts(
            repo.id,
            encode_sync_state_update(new_checkpoint, new_mapping),
        )

        return {
            "success": True,
            "code": SYNCED,
            "message": f"Synced {len(issues)} issues",
            "stats": stats,
        }

    async def acquire_lease(self, request: AcquireLeaseInput) -> dict[str, Any]:
        """Acquire a lease on a GitHub issue for the current Kagan instance."""
        if request.issue_number is None:
            return self._error(
                GH_ISSUE_REQUIRED,
                "issue_number is required",
                "Provide the GitHub issue number to acquire lease for",
            )
        issue_number, issue_error = self._coerce_positive_int(
            value=request.issue_number,
            field_name="issue_number",
            invalid_code=GH_ISSUE_NUMBER_INVALID,
        )
        if issue_error is not None:
            return issue_error
        assert issue_number is not None

        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        lease_enforced = load_lease_enforcement_state(repo.scripts)
        repo_context, connection_error = self._resolve_connected_repo_context(
            repo,
            require_owner_repo=lease_enforced,
        )
        if connection_error is not None:
            return connection_error
        assert repo_context is not None

        if not lease_enforced:
            return {
                "success": True,
                "code": LEASE_ENFORCEMENT_DISABLED,
                "message": "Lease enforcement disabled for this repository; skipping acquire.",
                "hint": (
                    "Set kagan.github.lease_enforcement=true in repo scripts to re-enable lease "
                    "coordination."
                ),
                "holder": None,
            }

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        github_user = await asyncio.to_thread(self._gh.run_gh_auth_username, gh_path)
        result = await asyncio.to_thread(
            self._gh.acquire_lease,
            gh_path,
            repo.path,
            str(repo_context["owner"]),
            str(repo_context["repo_name"]),
            issue_number,
            github_user=github_user,
            force_takeover=bool(request.force_takeover),
        )

        if result.success:
            return {
                "success": True,
                "code": result.code,
                "message": result.message,
                "holder": result.holder.to_dict() if result.holder else None,
            }

        response: dict[str, Any] = {
            "success": False,
            "code": result.code,
            "message": result.message,
        }
        if result.code == LEASE_HELD_BY_OTHER:
            if result.holder is not None:
                response["holder"] = result.holder.to_dict()
                response["hint"] = (
                    f"Issue #{issue_number} is locked by another instance. "
                    "Use force_takeover=true to take over the lease."
                )
            else:
                response["hint"] = (
                    "Lease holder metadata could not be verified. "
                    "Retry, or use force_takeover=true to proceed."
                )
        return response

    async def release_lease(self, request: ReleaseLeaseInput) -> dict[str, Any]:
        """Release a lease on a GitHub issue."""
        if request.issue_number is None:
            return self._error(
                GH_ISSUE_REQUIRED,
                "issue_number is required",
                "Provide the GitHub issue number to release lease for",
            )
        issue_number, issue_error = self._coerce_positive_int(
            value=request.issue_number,
            field_name="issue_number",
            invalid_code=GH_ISSUE_NUMBER_INVALID,
        )
        if issue_error is not None:
            return issue_error
        assert issue_number is not None

        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        lease_enforced = load_lease_enforcement_state(repo.scripts)
        repo_context, connection_error = self._resolve_connected_repo_context(
            repo,
            require_owner_repo=lease_enforced,
        )
        if connection_error is not None:
            return connection_error
        assert repo_context is not None

        if not lease_enforced:
            return {
                "success": True,
                "code": LEASE_ENFORCEMENT_DISABLED,
                "message": "Lease enforcement disabled for this repository; skipping release.",
            }

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        result = await asyncio.to_thread(
            self._gh.release_lease,
            gh_path,
            repo.path,
            str(repo_context["owner"]),
            str(repo_context["repo_name"]),
            issue_number,
        )
        return {
            "success": result.success,
            "code": result.code,
            "message": result.message,
        }

    async def get_lease_state(self, request: GetLeaseStateInput) -> dict[str, Any]:
        """Get the current lease state for a GitHub issue."""
        if request.issue_number is None:
            return self._error(
                GH_ISSUE_REQUIRED,
                "issue_number is required",
                "Provide the GitHub issue number to check lease state for",
            )
        issue_number, issue_error = self._coerce_positive_int(
            value=request.issue_number,
            field_name="issue_number",
            invalid_code=GH_ISSUE_NUMBER_INVALID,
        )
        if issue_error is not None:
            return issue_error
        assert issue_number is not None

        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        lease_enforced = load_lease_enforcement_state(repo.scripts)
        repo_context, connection_error = self._resolve_connected_repo_context(
            repo,
            require_owner_repo=lease_enforced,
        )
        if connection_error is not None:
            return connection_error
        assert repo_context is not None

        if not lease_enforced:
            return {
                "success": True,
                "code": LEASE_STATE_OK,
                "state": {
                    "is_locked": False,
                    "is_held_by_current_instance": False,
                    "can_acquire": True,
                    "requires_takeover": False,
                    "holder": None,
                    "enforcement_enabled": False,
                },
            }

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        state, error = await asyncio.to_thread(
            self._gh.get_lease_state,
            gh_path,
            repo.path,
            str(repo_context["owner"]),
            str(repo_context["repo_name"]),
            issue_number,
        )

        if error:
            return {
                "success": False,
                "code": LEASE_STATE_ERROR,
                "message": f"Failed to get lease state: {error}",
            }

        if state is None:
            return {
                "success": False,
                "code": LEASE_STATE_ERROR,
                "message": "Failed to get lease state",
            }

        return {
            "success": True,
            "code": LEASE_STATE_OK,
            "state": {
                "is_locked": state.is_locked,
                "is_held_by_current_instance": state.is_held_by_current_instance,
                "can_acquire": state.can_acquire,
                "requires_takeover": state.requires_takeover,
                "holder": state.holder.to_dict() if state.holder else None,
            },
        }

    async def validate_review_transition(
        self,
        request: ValidateReviewTransitionInput,
    ) -> dict[str, Any]:
        """Validate REVIEW transition guardrails for GitHub-connected repos."""
        task_id = self._non_empty_str(request.task_id)
        project_id = self._non_empty_str(request.project_id)
        if not task_id or not project_id:
            return {"allowed": True}

        try:
            repos = await self._core.get_project_repos(project_id)
        except Exception as exc:
            # Explicit resilience boundary: guardrail verification must fail closed.
            return self._review_guardrail_check_failed(str(exc))

        if not repos:
            return {"allowed": True}

        connected_repos: list[dict[str, Any]] = []
        for repo in repos:
            connection_state = load_connection_state(repo.scripts)
            if not connection_state.raw_value:
                continue

            connection = connection_state.normalized
            if connection is None:
                return self._review_guardrail_check_failed("invalid GitHub connection metadata")

            connected_repos.append(
                {
                    "repo": repo,
                    "connection": connection,
                    "pr_mapping": load_pr_mapping_state(repo.scripts),
                    "issue_mapping": load_issue_mapping_state(repo.scripts),
                    "lease_enforced": load_lease_enforcement_state(repo.scripts),
                }
            )

        if not connected_repos:
            return {"allowed": True}

        owner_candidates = [
            repo_ctx
            for repo_ctx in connected_repos
            if repo_ctx["pr_mapping"].has_pr(task_id)
            or repo_ctx["issue_mapping"].get_issue_number(task_id) is not None
        ]

        if not owner_candidates:
            try:
                owner_candidates = await self._resolve_workspace_owner_candidates(
                    task_id=task_id,
                    connected_repos=connected_repos,
                )
            except Exception as exc:
                return self._review_guardrail_check_failed(str(exc))

        if not owner_candidates:
            # Task is not scoped to a GitHub-connected repo; skip GitHub guardrails.
            return {"allowed": True}

        missing_pr_repos: list[str] = []
        lease_conflicts: list[str] = []
        gh_path: str | None = None

        for repo_ctx in owner_candidates:
            repo = repo_ctx["repo"]
            repo_label = self._repo_identifier(repo)
            if not repo_ctx["pr_mapping"].has_pr(task_id):
                missing_pr_repos.append(repo_label)
                continue

            issue_number = repo_ctx["issue_mapping"].get_issue_number(task_id)
            if issue_number is None or not bool(repo_ctx.get("lease_enforced", True)):
                continue

            owner_repo = resolve_owner_repo(repo_ctx["connection"])
            if owner_repo is None:
                return self._review_guardrail_check_failed(
                    f"{repo_label}: GitHub connection metadata missing owner/repo"
                )
            owner, repo_name = owner_repo

            if gh_path is None:
                gh_path, gh_error = self._gh.resolve_gh_cli_path()
                if gh_error is not None:
                    detail = (
                        str(gh_error.get("message"))
                        if isinstance(gh_error.get("message"), str)
                        else "GitHub CLI (gh) is unavailable"
                    )
                    return self._review_guardrail_check_failed(f"{repo_label}: {detail}")
            assert gh_path is not None

            state, error = await asyncio.to_thread(
                self._gh.get_lease_state,
                gh_path,
                repo.path,
                owner,
                repo_name,
                issue_number,
            )
            if error:
                return self._review_guardrail_check_failed(f"{repo_label}: {error}")
            if state is None:
                return self._review_guardrail_check_failed(
                    f"{repo_label}: lease state could not be determined"
                )
            if state.is_locked and not state.is_held_by_current_instance:
                holder_info = ""
                if state.holder:
                    holder_info = f" (held by {state.holder.instance_id})"
                lease_conflicts.append(f"{repo_label}{holder_info}")

        if missing_pr_repos:
            repo_list = ", ".join(sorted(set(missing_pr_repos)))
            return {
                "allowed": False,
                "code": REVIEW_BLOCKED_NO_PR,
                "message": (
                    "REVIEW transition blocked: no linked PR for repo(s): "
                    f"{repo_list}. Create or link PRs before requesting review."
                ),
                "hint": (
                    "Use create_pr_for_task or link_pr_to_task with repo_id for each repo."
                ),
            }

        if lease_conflicts:
            conflict_details = ", ".join(sorted(set(lease_conflicts)))
            return {
                "allowed": False,
                "code": REVIEW_BLOCKED_LEASE,
                "message": (
                    "REVIEW transition blocked: lease held by another instance for repo(s): "
                    f"{conflict_details}. Wait for lease release before requesting review."
                ),
                "hint": "The issue is being worked on by another Kagan instance.",
            }

        return {"allowed": True}

    async def create_pr_for_task(self, request: CreatePrForTaskInput) -> dict[str, Any]:
        """Create a PR for a task and link it."""
        if not request.task_id:
            return self._error(
                GH_TASK_REQUIRED,
                "task_id is required",
                "Provide the task ID to create a PR for",
            )

        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        repo_context, connection_error = self._resolve_connected_repo_context(repo)
        if connection_error is not None:
            return connection_error
        assert repo_context is not None

        connection = repo_context["connection"]
        base_branch = connection.get("default_branch", "main")

        task = await self._core.get_task(request.task_id)
        if task is None:
            return self._error(
                GH_TASK_REQUIRED,
                f"Task not found: {request.task_id}",
                "Verify the task_id exists",
            )

        workspaces = await self._core.list_workspaces(task_id=request.task_id)
        if not workspaces:
            return self._error(
                GH_WORKSPACE_REQUIRED,
                "Task has no workspace",
                "Create a workspace for the task first",
            )

        workspace = workspaces[0]
        head_branch = workspace.branch_name

        pr_title = request.title or task.title
        pr_body = request.body or task.description or ""

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        pr_data, error = await asyncio.to_thread(
            self._gh.run_gh_pr_create,
            gh_path,
            repo.path,
            head_branch=head_branch,
            base_branch=base_branch,
            title=pr_title,
            body=pr_body,
            draft=bool(request.draft),
        )

        if error:
            return {
                "success": False,
                "code": GH_PR_CREATE_FAILED,
                "message": f"Failed to create PR: {error}",
                "hint": "Check that changes are pushed and the branch exists on GitHub",
            }

        if pr_data is None:
            return {
                "success": False,
                "code": GH_PR_CREATE_FAILED,
                "message": "Failed to create PR: no data returned",
            }

        assert request.task_id is not None
        await self._persist_pr_link(repo, request.task_id, pr_data)

        return {
            "success": True,
            "code": PR_CREATED,
            "message": f"Created PR #{pr_data.number}",
            "pr": {
                "number": pr_data.number,
                "url": pr_data.url,
                "state": pr_data.state,
                "head_branch": pr_data.head_branch,
                "base_branch": pr_data.base_branch,
                "is_draft": pr_data.is_draft,
            },
        }

    async def link_pr_to_task(self, request: LinkPrToTaskInput) -> dict[str, Any]:
        """Link an existing PR to a task."""
        if not request.task_id:
            return self._error(
                GH_TASK_REQUIRED,
                "task_id is required",
                "Provide the task ID to link the PR to",
            )

        if request.pr_number is None:
            return self._error(
                GH_PR_NUMBER_REQUIRED,
                "pr_number is required",
                "Provide the PR number to link",
            )
        pr_number, pr_number_error = self._coerce_positive_int(
            value=request.pr_number,
            field_name="pr_number",
            invalid_code=GH_PR_NUMBER_INVALID,
        )
        if pr_number_error is not None:
            return pr_number_error
        assert pr_number is not None

        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        _, connection_error = self._resolve_connected_repo_context(repo)
        if connection_error is not None:
            return connection_error

        task = await self._core.get_task(request.task_id)
        if task is None:
            return self._error(
                GH_TASK_REQUIRED,
                f"Task not found: {request.task_id}",
                "Verify the task_id exists",
            )

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        pr_data, error = await asyncio.to_thread(
            self._gh.run_gh_pr_view,
            gh_path,
            repo.path,
            pr_number,
        )
        if error:
            return {
                "success": False,
                "code": GH_PR_NOT_FOUND,
                "message": f"Failed to find PR #{pr_number}: {error}",
                "hint": "Verify the PR exists and you have access to it",
            }

        if pr_data is None:
            return {
                "success": False,
                "code": GH_PR_NOT_FOUND,
                "message": f"PR #{pr_number} not found",
            }

        assert request.task_id is not None
        await self._persist_pr_link(repo, request.task_id, pr_data)

        return {
            "success": True,
            "code": PR_LINKED,
            "message": f"Linked PR #{pr_data.number} to task {request.task_id}",
            "pr": {
                "number": pr_data.number,
                "url": pr_data.url,
                "state": pr_data.state,
                "head_branch": pr_data.head_branch,
                "base_branch": pr_data.base_branch,
                "is_draft": pr_data.is_draft,
            },
        }

    async def reconcile_pr_status(self, request: ReconcilePrStatusInput) -> dict[str, Any]:
        """Reconcile PR status for a task and apply deterministic board transitions."""
        if not request.task_id:
            return self._error(
                GH_TASK_REQUIRED,
                "task_id is required",
                "Provide the task ID to reconcile PR status for",
            )

        repo, resolve_error = await self._resolve_connect_target(
            request.project_id,
            request.repo_id,
        )
        if resolve_error is not None:
            return resolve_error
        assert repo is not None

        _, connection_error = self._resolve_connected_repo_context(repo)
        if connection_error is not None:
            return connection_error

        pr_mapping = load_pr_mapping_state(repo.scripts)
        pr_link = pr_mapping.get_pr(request.task_id)
        if pr_link is None:
            return self._error(
                GH_NO_LINKED_PR,
                f"Task {request.task_id} has no linked PR",
                "Use create_pr_for_task or link_pr_to_task first",
            )

        task = await self._core.get_task(request.task_id)
        if task is None:
            return self._error(
                GH_TASK_REQUIRED,
                f"Task not found: {request.task_id}",
                "Verify the task_id exists",
            )

        gh_path, gh_error = self._gh.resolve_gh_cli_path()
        if gh_error is not None:
            return gh_error
        assert gh_path is not None

        pr_data, error = await asyncio.to_thread(
            self._gh.run_gh_pr_view,
            gh_path,
            repo.path,
            pr_link.pr_number,
        )
        if error:
            return {
                "success": False,
                "code": GH_PR_NOT_FOUND,
                "message": f"Failed to fetch PR #{pr_link.pr_number}: {error}",
                "hint": (
                    "Check network connectivity and GitHub access. Retry the reconcile operation."
                ),
            }

        if pr_data is None:
            return {
                "success": False,
                "code": GH_PR_NOT_FOUND,
                "message": f"PR #{pr_link.pr_number} not found",
                "hint": "The PR may have been deleted. Consider unlinking and creating a new PR.",
            }

        pr_state_changed = pr_data.state != pr_link.pr_state
        task_status_changed = False
        previous_task_status = task.status
        new_task_status = task.status

        if pr_state_changed:
            pr_mapping.update_pr_state(request.task_id, pr_data.state)
            await self._core.update_repo_scripts(repo.id, encode_pr_mapping_update(pr_mapping))

        if pr_data.state == "MERGED":
            if task.status != TaskStatus.DONE:
                await self._core.update_task_fields(request.task_id, status=TaskStatus.DONE)
                task_status_changed = True
                new_task_status = TaskStatus.DONE
        elif pr_data.state == "CLOSED":
            if task.status not in {TaskStatus.DONE, TaskStatus.IN_PROGRESS}:
                await self._core.update_task_fields(request.task_id, status=TaskStatus.IN_PROGRESS)
                task_status_changed = True
                new_task_status = TaskStatus.IN_PROGRESS

        return {
            "success": True,
            "code": PR_STATUS_RECONCILED,
            "message": self.build_reconcile_message(
                pr_data.number,
                pr_data.state,
                task_status_changed,
            ),
            "pr": {
                "number": pr_data.number,
                "url": pr_data.url,
                "state": pr_data.state,
                "previous_state": pr_link.pr_state,
                "state_changed": pr_state_changed,
            },
            "task": {
                "id": request.task_id,
                "status": new_task_status.value,
                "previous_status": previous_task_status.value,
                "status_changed": task_status_changed,
            },
        }

    @staticmethod
    def build_reconcile_message(pr_number: int, pr_state: str, task_changed: bool) -> str:
        """Build a human-readable reconcile result message."""
        if pr_state == "MERGED":
            if task_changed:
                return f"PR #{pr_number} merged. Task moved to DONE."
            return f"PR #{pr_number} merged. Task already DONE."
        if pr_state == "CLOSED":
            if task_changed:
                return f"PR #{pr_number} closed without merge. Task moved to IN_PROGRESS."
            return f"PR #{pr_number} closed without merge. Task status unchanged."
        return f"PR #{pr_number} is open. No task status change."

    @staticmethod
    def _review_guardrail_check_failed(detail: str) -> dict[str, Any]:
        return {
            "allowed": False,
            "code": REVIEW_GUARDRAIL_CHECK_FAILED,
            "message": "REVIEW transition blocked: failed to verify GitHub guardrails.",
            "hint": f"Resolve GitHub plugin health and retry. Details: {detail}",
        }

    async def _resolve_workspace_owner_candidates(
        self,
        *,
        task_id: str,
        connected_repos: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        workspaces = await self._core.list_workspaces(task_id=task_id)
        workspace_repo_ids: set[str] = set()
        for workspace in workspaces:
            workspace_repos = await self._core.get_workspace_repos(workspace.id)
            for workspace_repo in workspace_repos:
                if not isinstance(workspace_repo, dict):
                    continue
                repo_id = workspace_repo.get("repo_id")
                if isinstance(repo_id, str) and repo_id:
                    workspace_repo_ids.add(repo_id)

        if not workspace_repo_ids:
            return []

        owner_candidates: list[dict[str, Any]] = []
        for repo_ctx in connected_repos:
            repo = repo_ctx["repo"]
            repo_id = repo.id if hasattr(repo, "id") else None
            if isinstance(repo_id, str) and repo_id in workspace_repo_ids:
                owner_candidates.append(repo_ctx)
        return owner_candidates

    @staticmethod
    def _repo_identifier(repo: Any) -> str:
        repo_id = repo.id if hasattr(repo, "id") else None
        if isinstance(repo_id, str) and repo_id:
            return repo_id
        repo_name = repo.name if hasattr(repo, "name") else None
        if isinstance(repo_name, str) and repo_name:
            return repo_name
        return "<unknown-repo>"

    async def _resolve_connect_target(
        self,
        project_id: str | None,
        repo_id: str | None,
    ) -> tuple[Any | None, dict[str, Any] | None]:
        project_id = self._non_empty_str(project_id)
        repo_id = self._non_empty_str(repo_id)
        if not project_id:
            return None, self._error(
                GH_PROJECT_REQUIRED,
                "project_id is required",
                "Provide a valid project_id parameter",
            )

        project = await self._core.get_project(project_id)
        if not project:
            return None, self._error(
                GH_PROJECT_REQUIRED,
                f"Project not found: {project_id}",
                "Verify the project_id exists",
            )

        repos = await self._core.get_project_repos(project_id)
        if not repos:
            return None, self._error(
                GH_REPO_REQUIRED,
                "Project has no repositories",
                "Add a repository to the project first",
            )

        if len(repos) == 1:
            target_repo = repos[0]
            target_repo_id = target_repo.id if hasattr(target_repo, "id") else None
            if repo_id and repo_id != target_repo_id:
                expected = target_repo_id if isinstance(target_repo_id, str) else "<unknown>"
                return None, self._error(
                    GH_REPO_REQUIRED,
                    f"Repo not found in project: {repo_id}",
                    f"Project has a single repo ({expected}). Use that repo_id or omit repo_id.",
                )
            return target_repo, None

        if not repo_id:
            return None, self._error(
                GH_REPO_REQUIRED,
                "repo_id required for multi-repo projects",
                f"Project has {len(repos)} repos. Specify repo_id explicitly.",
            )

        target_repo = next((repo for repo in repos if repo.id == repo_id), None)
        if target_repo is None:
            return None, self._error(
                GH_REPO_REQUIRED,
                f"Repo not found in project: {repo_id}",
                "Verify the repo_id belongs to this project",
            )

        return target_repo, None

    def _resolve_connected_repo_context(
        self,
        repo: Any,
        *,
        require_owner_repo: bool = False,
    ) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
        connection_state = load_connection_state(repo.scripts)
        if not connection_state.raw_value:
            return None, self._error(
                GH_NOT_CONNECTED,
                "Repository is not connected to GitHub",
                "Run connect_repo first to establish GitHub connection",
            )

        connection = connection_state.normalized
        if connection is None:
            return None, self._error(
                GH_REPO_METADATA_INVALID,
                "Stored GitHub connection metadata is invalid",
                "Reconnect the repository using connect_repo.",
            )

        context: dict[str, Any] = {"connection": connection}
        if require_owner_repo:
            owner_repo = resolve_owner_repo(connection)
            if owner_repo is None:
                return None, self._error(
                    GH_REPO_METADATA_INVALID,
                    "Stored GitHub connection metadata is incomplete",
                    "Reconnect the repository to refresh owner/repo metadata.",
                )
            owner, repo_name = owner_repo
            context["owner"] = owner
            context["repo_name"] = repo_name

        return context, None

    async def _load_mapped_tasks(self, mapping: IssueMapping) -> dict[str, dict[str, Any]]:
        tasks: dict[str, dict[str, Any]] = {}
        for task_id in mapping.task_to_issue:
            task = await self._core.get_task(task_id)
            if task:
                tasks[task_id] = {
                    "title": task.title,
                    "status": task.status,
                    "task_type": task.task_type,
                }
        return tasks

    @staticmethod
    def _coerce_positive_int(
        *,
        value: object,
        field_name: str,
        invalid_code: str,
    ) -> tuple[int | None, dict[str, Any] | None]:
        parsed_value: int | None = None
        if isinstance(value, bool):
            parsed_value = None
        elif isinstance(value, int):
            parsed_value = value
        elif isinstance(value, str):
            raw_value = value.strip()
            if raw_value:
                try:
                    parsed_value = int(raw_value)
                except ValueError:
                    parsed_value = None

        if parsed_value is None or parsed_value <= 0:
            return None, GitHubPluginUseCases._error(
                invalid_code,
                f"{field_name} must be a positive integer",
                f"Provide a numeric {field_name} value like 123",
            )
        return parsed_value, None

    @staticmethod
    def _non_empty_str(value: object) -> str | None:
        if not isinstance(value, str):
            return None
        normalized = value.strip()
        return normalized if normalized else None

    async def _persist_pr_link(self, repo: Any, task_id: str, pr_data: Any) -> None:
        pr_mapping = load_pr_mapping_state(repo.scripts)
        pr_mapping.link_pr(
            task_id=task_id,
            pr_number=pr_data.number,
            pr_url=pr_data.url,
            pr_state=pr_data.state,
            head_branch=pr_data.head_branch,
            base_branch=pr_data.base_branch,
            linked_at=utc_now().isoformat(),
        )
        await self._core.update_repo_scripts(repo.id, encode_pr_mapping_update(pr_mapping))

    @staticmethod
    def _error(code: str, message: str, hint: str) -> dict[str, Any]:
        return {"success": False, "code": code, "message": message, "hint": hint}


__all__ = [
    "GH_ISSUE_NUMBER_INVALID",
    "GH_ISSUE_REQUIRED",
    "GH_NOT_CONNECTED",
    "GH_NO_LINKED_PR",
    "GH_PR_CREATE_FAILED",
    "GH_PR_NOT_FOUND",
    "GH_PR_NUMBER_INVALID",
    "GH_PR_NUMBER_REQUIRED",
    "GH_SYNC_FAILED",
    "GH_TASK_REQUIRED",
    "GH_WORKSPACE_REQUIRED",
    "PR_STATUS_RECONCILED",
    "GitHubPluginUseCases",
]
