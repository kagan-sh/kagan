"""Unified task modal for viewing, editing, and creating tasks."""

from __future__ import annotations

import contextlib
from enum import Enum, StrEnum, auto
from typing import TYPE_CHECKING, Final

from sqlalchemy.exc import OperationalError
from textual import on
from textual.containers import Horizontal, Vertical
from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widgets import Button, Footer, Input, Label, Rule, Select, Static, TextArea

from kagan.core.adapters.db.repositories.base import RepositoryClosing
from kagan.core.models.enums import VALID_PAIR_BACKENDS, TaskPriority, TaskStatus, TaskType
from kagan.tui.keybindings import TASK_DETAILS_BINDINGS
from kagan.tui.ui.modals.base import KaganModalScreen
from kagan.tui.ui.modals.description_editor import DescriptionEditorModal
from kagan.tui.ui.user_messages import task_deleted_close_message
from kagan.tui.ui.utils import copy_with_notification, safe_query_one
from kagan.tui.ui.widgets.base import (
    AcceptanceCriteriaArea,
    AgentBackendSelect,
    BaseBranchInput,
    PairTerminalBackendSelect,
    PrioritySelect,
    StatusSelect,
    TaskTypeSelect,
    TitleInput,
)
from kagan.tui.ui.widgets.task_mentions import (
    TaskMentionArea,
    TaskMentionComplete,
    TaskMentionItem,
    handle_mention_completed,
    handle_mention_dismissed,
    handle_mention_key,
    handle_mention_query,
)
from kagan.tui.ui.widgets.workspace_repos import WorkspaceReposWidget

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.adapters.db.schema import Task


TaskUpdateDict = dict[str, object]
_TASK_DETAILS_LOAD_ERRORS = (RepositoryClosing, OperationalError, RuntimeError, ValueError)
_DEFAULT_AGENT_KEY: Final = "claude"
_DEFAULT_PAIR_TERMINAL_BACKEND: Final = "tmux"


class ModalAction(Enum):
    DELETE = auto()


class BindingAction(StrEnum):
    EXPAND_DESCRIPTION = "expand_description"
    FULL_EDITOR = "full_editor"
    SAVE = "save"


class TaskDetailsModal(KaganModalScreen[ModalAction | TaskUpdateDict | None]):
    """Unified modal for viewing, editing, and creating tasks."""

    editing = reactive(False)

    BINDINGS = TASK_DETAILS_BINDINGS

    def __init__(
        self,
        task: Task | None = None,
        *,
        start_editing: bool = False,
        initial_type: TaskType | None = None,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._task_model = task
        self.is_create = task is None
        self._initial_type = initial_type

        self._is_done = False
        self._mention_items: list[TaskMentionItem] = []
        self._mention_complete: TaskMentionComplete | None = None
        if task is not None:
            status = task.status
            self._is_done = status == TaskStatus.DONE

        if self._is_done:
            start_editing = False
        self._initial_editing = self.is_create or start_editing

    def on_mount(self) -> None:
        task_changed_signal = getattr(self.kagan_app, "task_changed_signal", None)
        if task_changed_signal is not None:
            task_changed_signal.subscribe(self, self._on_task_changed)
        if self.is_create:
            self.add_class("create-mode")
        self.editing = self._initial_editing
        self._sync_pair_terminal_visibility()
        if self.editing:
            if title_input := safe_query_one(self, "#title-input", Input):
                title_input.focus()
        if self._task_model and not self.is_create:
            self.run_worker(
                self._load_workspace_repos,
                group="task-details-workspace-repos",
                exclusive=True,
                exit_on_error=False,
            )
        if self.editing:
            self.run_worker(
                self._load_mention_items,
                group="task-details-mention-items",
                exclusive=True,
                exit_on_error=False,
            )

    def on_unmount(self) -> None:
        task_changed_signal = getattr(self.kagan_app, "task_changed_signal", None)
        if task_changed_signal is None:
            return
        with contextlib.suppress(Exception):
            task_changed_signal.unsubscribe(self)

    async def _on_task_changed(self, task_id: str) -> None:
        if not self.is_mounted or self._task_model is None or task_id != self._task_model.id:
            return
        self.run_worker(
            self._refresh_task_presence,
            group="task-details-task-refresh",
            exclusive=True,
            exit_on_error=False,
        )

    async def _refresh_task_presence(self) -> None:
        if self._task_model is None:
            return
        try:
            latest = await self.ctx.api.get_task(self._task_model.id)
        except _TASK_DETAILS_LOAD_ERRORS:
            return
        if latest is None:
            self.notify(task_deleted_close_message("task details"), severity="warning")
            self.dismiss(None)
            return
        self._task_model = latest

    def compose(self) -> ComposeResult:
        with Vertical(id="task-details-container"):
            yield Label(
                self._get_modal_title(),
                classes="modal-title",
                id="modal-title-label",
            )
            yield Rule(line_style="heavy")

            yield from self._compose_badge_row()

            yield from self._compose_edit_fields_row()
            yield from self._compose_pair_terminal_row()

            if self.is_create:
                with Vertical(classes="form-field edit-fields", id="status-field"):
                    yield Label("Status:", classes="form-label")
                    yield StatusSelect()

            yield Rule()

            yield from self._compose_title_field()

            yield Rule()

            yield from self._compose_description_field()

            yield from self._compose_acceptance_criteria()

            yield from self._compose_base_branch_field()

            yield from self._compose_workspace_repos_section()

            yield from self._compose_meta_row()

            yield Rule()

            yield from self._compose_buttons()

        yield Footer(show_command_palette=False)

    def _compose_badge_row(self) -> ComposeResult:
        """Compose the badge row for view mode."""
        with Horizontal(classes="badge-row view-only", id="badge-row"):
            yield Label(
                self._get_priority_label(),
                classes=f"badge {self._get_priority_class()}",
                id="priority-badge",
            )
            yield Label(
                self._get_type_label(),
                classes="badge badge-type",
                id="type-badge",
            )
            yield Label(
                self._format_status(
                    self._task_model.status if self._task_model else TaskStatus.BACKLOG
                ),
                classes="badge badge-status",
                id="status-badge",
            )
            if agent_label := self._get_agent_label():
                yield Label(agent_label, classes="badge badge-agent", id="agent-badge")

    def _compose_edit_fields_row(self) -> ComposeResult:
        """Compose the edit fields row (priority, type, agent)."""
        current_priority = self._task_model.priority if self._task_model else TaskPriority.MEDIUM

        current_type = self._initial_type or (
            self._task_model.task_type if self._task_model else TaskType.PAIR
        )

        with Horizontal(classes="field-row edit-fields", id="edit-fields-row"):
            with Vertical(classes="form-field field-third"):
                yield Label("Priority:", classes="form-label")
                yield PrioritySelect(value=current_priority)

            with Vertical(classes="form-field field-third"):
                yield Label("Type:", classes="form-label")
                yield TaskTypeSelect(value=current_type)

            with Vertical(classes="form-field field-third"):
                yield Label("Agent:", classes="form-label")
                agent_options = self._build_agent_options()
                current_backend = self._get_agent_backend_value()
                yield AgentBackendSelect(options=agent_options, value=current_backend)

    def _compose_pair_terminal_row(self) -> ComposeResult:
        with Horizontal(classes="field-row edit-fields", id="pair-terminal-row"):
            with Vertical(classes="form-field field-third", id="pair-terminal-field"):
                yield Label("Terminal:", classes="form-label")
                yield PairTerminalBackendSelect(value=self._get_pair_terminal_backend_value())

    def _compose_title_field(self) -> ComposeResult:
        """Compose the title field."""
        title = self._task_model.title if self._task_model else ""

        yield Label("Title", classes="section-title view-only", id="title-section-label")
        yield Static(title, classes="task-title view-only", id="title-display", markup=False)

        with Vertical(classes="form-field edit-fields", id="title-field"):
            yield TitleInput(value=title)

    def _compose_description_field(self) -> ComposeResult:
        """Compose the description field."""
        with Horizontal(classes="description-header"):
            yield Label("Description", classes="section-title")
            yield Static("", classes="header-spacer")
            expand_text = "[f] Expand" if not self.editing else "[F5] Full Editor"
            yield Static(expand_text, classes="expand-hint", id="expand-btn")

        description = (
            self._task_model.description if self._task_model else ""
        ) or "(No description)"
        yield Static(
            description,
            classes="task-description view-only",
            id="description-content",
            markup=False,
        )

        with Vertical(classes="form-field edit-fields", id="description-field"):
            yield TaskMentionArea(
                text=self._task_model.description if self._task_model else "",
                show_line_numbers=True,
                id="description-input",
            )
            yield TaskMentionComplete(id="mention-complete")

    def _compose_acceptance_criteria(self) -> ComposeResult:
        """Compose the acceptance criteria section."""
        if self._task_model and self._task_model.acceptance_criteria:
            with Vertical(classes="acceptance-criteria-section view-only", id="ac-section"):
                yield Label("Acceptance Criteria", classes="section-title")
                for criterion in self._task_model.acceptance_criteria:
                    yield Static(f"  - {criterion}", classes="ac-item")

        with Vertical(classes="form-field edit-fields", id="ac-field"):
            yield Label("Acceptance Criteria (one per line):", classes="form-label")
            criteria = self._task_model.acceptance_criteria if self._task_model else []
            yield AcceptanceCriteriaArea(criteria=criteria)

    def _compose_base_branch_field(self) -> ComposeResult:
        base_branch = self._task_model.base_branch if self._task_model else None

        if base_branch:
            with Vertical(classes="base-branch-section view-only", id="base-branch-view"):
                yield Label(f"Branch: {base_branch}", classes="field-value")

        with Vertical(classes="form-field edit-fields", id="base-branch-field"):
            yield Label("Base Branch:", classes="form-label")
            yield BaseBranchInput(value=base_branch or "")

    def _compose_workspace_repos_section(self) -> ComposeResult:
        if self.is_create or not self._task_model:
            return
        with Vertical(classes="workspace-repos-section view-only", id="workspace-repos-section"):
            yield Label("Workspace Repos", classes="section-title")
            yield Static("Loading workspace repos...", id="workspace-repos-loading")
        yield Rule()

    async def _load_workspace_repos(self) -> None:
        if not self._task_model:
            return
        try:
            workspace_service = self.ctx.api.workspace_service
            workspaces = await workspace_service.list_workspaces(task_id=self._task_model.id)
        except _TASK_DETAILS_LOAD_ERRORS as exc:
            self.log(
                "Workspace repo lookup failed",
                task_id=self._task_model.id,
                error=str(exc),
            )
            return

        container = safe_query_one(self, "#workspace-repos-section", Vertical)
        loading = safe_query_one(self, "#workspace-repos-loading", Static)
        if not container or not loading:
            return

        if not workspaces:
            loading.update("No workspace yet")
            return

        loading.display = False
        await container.mount(
            WorkspaceReposWidget(
                workspaces[0].id,
                workspace_service=self.ctx.api.workspace_service,
                diff_service=self.ctx.api.diff_service,
            )
        )

    def _compose_meta_row(self) -> ComposeResult:
        """Compose the metadata row."""
        with Horizontal(classes="meta-row", id="meta-row"):
            if self._task_model:
                created = f"Created: {self._task_model.created_at:%Y-%m-%d %H:%M}"
                updated = f"Updated: {self._task_model.updated_at:%Y-%m-%d %H:%M}"
                yield Label(created, classes="task-meta")
                yield Static("  |  ", classes="meta-separator")
                yield Label(updated, classes="task-meta")

    def _compose_buttons(self) -> ComposeResult:
        """Compose the button rows."""
        with Horizontal(classes="button-row view-only", id="view-buttons"):
            yield Button("[Esc] Close", id="close-btn")
            yield Button("[e] Edit", id="edit-btn", disabled=self._is_done)
            yield Button("[d] Delete", variant="error", id="delete-btn")

        with Horizontal(classes="button-row edit-fields", id="edit-buttons"):
            yield Button("[F2] Save", variant="primary", id="save-btn")
            yield Button("[Esc] Cancel", id="cancel-btn")

    def watch_editing(self, editing: bool) -> None:
        self.set_class(editing, "editing")
        self._sync_pair_terminal_visibility()

        if title_label := safe_query_one(self, "#modal-title-label", Label):
            title_label.update(self._get_modal_title())

        if expand_btn := safe_query_one(self, "#expand-btn", Static):
            expand_btn.update("[F5] Full Editor" if editing else "[f] Expand")

        self.refresh_bindings()

        if editing:
            self.run_worker(
                self._load_mention_items,
                group="task-details-mention-items",
                exclusive=True,
                exit_on_error=False,
            )
            if title_input := safe_query_one(self, "#title-input", Input):
                title_input.focus()

    @on(Select.Changed, "#type-select")
    def on_type_changed(self) -> None:
        self._sync_pair_terminal_visibility()

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        """Control which bindings are shown based on editing state.

        Returns True to show/enable, False to hide/disable, None for default.
        """
        try:
            binding_action = BindingAction(action)
        except ValueError:
            return True

        if binding_action is BindingAction.EXPAND_DESCRIPTION:
            return not self.editing
        return self.editing

    @on(Button.Pressed, "#edit-btn")
    def on_edit_btn(self) -> None:
        self.action_toggle_edit()

    @on(Button.Pressed, "#delete-btn")
    def on_delete_btn(self) -> None:
        self.action_delete()

    @on(Button.Pressed, "#close-btn")
    def on_close_btn(self) -> None:
        self.dismiss(None)

    @on(Button.Pressed, "#save-btn")
    def on_save_btn(self) -> None:
        self.action_save()

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel_btn(self) -> None:
        self.action_close_or_cancel()

    def action_toggle_edit(self) -> None:
        if self._is_done:
            self.app.notify("Done tasks cannot be edited", severity="warning")
            return
        if not self.editing and not self.is_create:
            self.editing = True

    def action_delete(self) -> None:
        if not self.editing and self._task_model:
            self.dismiss(ModalAction.DELETE)

    def action_close_or_cancel(self) -> None:
        """Escape always cancels/closes without saving."""
        self.dismiss(None)

    def action_save(self) -> None:
        if not self.editing:
            return
        result = self._validate_and_build_result()
        if result is not None:
            self._notify_if_backend_changed(result)
            self.dismiss(result)

    def action_copy(self) -> None:
        """Copy task details to clipboard."""
        if not self._task_model:
            self.app.notify("No task to copy", severity="warning")
            return
        content = f"#{self._task_model.short_id}: {self._task_model.title}"
        if self._task_model.description:
            content += f"\n\n{self._task_model.description}"
        copy_with_notification(self.app, content, "Task")

    def action_expand_description(self) -> None:
        """Expand description in read-only view (for view mode)."""
        if self.editing:
            self.action_full_editor()
            return
        description = self._task_model.description if self._task_model else ""
        modal = DescriptionEditorModal(
            description=description, readonly=True, title="View Description"
        )
        self.app.push_screen(modal)

    def action_full_editor(self) -> None:
        """Open full editor for description (for edit mode)."""
        if not self.editing:
            self.action_expand_description()
            return
        description_input = self.query_one("#description-input", TextArea)
        self.run_worker(
            self._open_full_description_editor(description_input),
            group="task-details-full-editor",
            exclusive=True,
            exit_on_error=False,
        )

    async def _open_full_description_editor(self, description_input: TextArea) -> None:
        result = await self.app.push_screen(
            DescriptionEditorModal(
                description=description_input.text,
                readonly=False,
                title="Edit Description",
                mention_items=self._mention_items,
            )
        )
        if result is not None:
            description_input.text = result

    async def _load_mention_items(self) -> None:
        try:
            ctx = self.ctx
        except RuntimeError:
            return
        project_id = self._task_model.project_id if self._task_model else ctx.active_project_id
        if project_id is None:
            return

        try:
            tasks = await ctx.api.list_tasks(project_id=project_id)
        except _TASK_DETAILS_LOAD_ERRORS as exc:
            self.log("Task mention lookup failed", project_id=project_id, error=str(exc))
            return
        current_id = self._task_model.id if self._task_model else None
        self._mention_items = [
            TaskMentionItem(task_id=task.id, title=task.title, status=task.status.value)
            for task in tasks
            if task.id != current_id
        ]
        if self._mention_complete is not None:
            self._mention_complete.set_items(self._mention_items)

    def _ensure_mention_complete(self) -> TaskMentionComplete:
        if self._mention_complete is None:
            try:
                self._mention_complete = self.query_one("#mention-complete", TaskMentionComplete)
            except NoMatches as exc:  # pragma: no cover - event ordering guard
                raise RuntimeError("Mention completion widget is not available") from exc
            self._mention_complete.set_items(self._mention_items)
        return self._mention_complete

    @on(TaskMentionArea.MentionQuery, "#description-input")
    def on_mention_query(self, event: TaskMentionArea.MentionQuery) -> None:
        if not self.editing:
            return
        handle_mention_query(self._ensure_mention_complete(), event.query)

    @on(TaskMentionArea.MentionDismissed, "#description-input")
    def on_mention_dismissed(self, event: TaskMentionArea.MentionDismissed) -> None:
        handle_mention_dismissed(self._mention_complete)

    @on(TaskMentionArea.MentionKey, "#description-input")
    def on_mention_key(self, event: TaskMentionArea.MentionKey) -> None:
        handle_mention_key(
            self._mention_complete,
            self.query_one("#description-input", TaskMentionArea),
            event.key,
        )

    @on(TaskMentionComplete.Completed)
    def on_mention_completed(self, event: TaskMentionComplete.Completed) -> None:
        handle_mention_completed(
            self._mention_complete,
            self.query_one("#description-input", TaskMentionArea),
            event.task_id,
        )

    def _get_modal_title(self) -> str:
        """Get the modal title based on current state."""
        if self.is_create:
            return "New Task"
        elif self.editing:
            return "Edit Task"
        else:
            return "Task Details"

    def _get_priority_label(self) -> str:
        """Get the priority label for display."""
        if not self._task_model:
            return "MED"
        return self._task_model.priority.label

    def _get_priority_class(self) -> str:
        if not self._task_model:
            return "badge-priority-medium"
        return f"badge-priority-{self._task_model.priority.css_class}"

    def _get_type_label(self) -> str:
        if not self._task_model:
            return "PAIR"
        if self._task_model.task_type == TaskType.AUTO:
            return "AUTO"
        return "PAIR"

    def _format_status(self, status: TaskStatus) -> str:
        return status.value.replace("_", " ")

    def _build_agent_options(self) -> list[tuple[str, str]]:
        from kagan.core.builtin_agents import BUILTIN_AGENTS

        kagan_app = self.kagan_app
        default_agent = self._get_default_agent_key()
        options: list[tuple[str, str]] = []
        seen: set[str] = set()

        if hasattr(kagan_app, "config") and kagan_app.config.agents:
            for name, agent in kagan_app.config.agents.items():
                if not agent.active:
                    continue
                label = self._format_agent_label(agent.name, name == default_agent)
                options.append((label, name))
                seen.add(name)

        for name, agent in BUILTIN_AGENTS.items():
            if name in seen:
                continue
            label = self._format_agent_label(agent.config.name, name == default_agent)
            options.append((label, name))

        options.sort(key=lambda item: 0 if item[1] == default_agent else 1)
        return options

    def _get_default_agent_key(self) -> str:
        if hasattr(self.kagan_app, "config"):
            return self.kagan_app.config.general.default_worker_agent
        return _DEFAULT_AGENT_KEY

    def _get_agent_backend_value(self) -> str:
        if self._task_model and self._task_model.agent_backend:
            return self._task_model.agent_backend
        return self._get_default_agent_key()

    def _get_agent_label(self) -> str:
        agent_key = self._get_agent_backend_value()
        if not agent_key:
            return ""
        if hasattr(self.kagan_app, "config"):
            agent_config = self.kagan_app.config.get_agent(agent_key)
            if agent_config:
                return self._format_agent_label(agent_config.name, False)
        from kagan.core.builtin_agents import get_builtin_agent

        if builtin := get_builtin_agent(agent_key):
            return self._format_agent_label(builtin.config.name, False)
        return self._format_agent_label(agent_key, False)

    def _get_default_pair_terminal_backend(self) -> str:
        if hasattr(self.kagan_app, "config"):
            configured = getattr(
                self.kagan_app.config.general,
                "default_pair_terminal_backend",
                _DEFAULT_PAIR_TERMINAL_BACKEND,
            )
            if configured in VALID_PAIR_BACKENDS:
                return configured
        return _DEFAULT_PAIR_TERMINAL_BACKEND

    def _get_pair_terminal_backend_value(self) -> str:
        task_backend = getattr(self._task_model, "terminal_backend", None)
        if isinstance(task_backend, str) and task_backend in VALID_PAIR_BACKENDS:
            return task_backend
        return self._get_default_pair_terminal_backend()

    def _is_pair_type_selected(self) -> bool:
        type_select = safe_query_one(self, "#type-select", Select)
        if type_select is None:
            return True
        selected = type_select.value
        if selected is Select.BLANK:
            return True
        return str(selected) == TaskType.PAIR.value

    def _sync_pair_terminal_visibility(self) -> None:
        pair_field = safe_query_one(self, "#pair-terminal-field", Vertical)
        pair_select = safe_query_one(self, "#pair-terminal-backend-select", Select)
        if pair_field is None or pair_select is None:
            return
        is_pair = self._is_pair_type_selected()
        pair_field.display = is_pair
        pair_select.disabled = not is_pair

    @staticmethod
    def _format_agent_label(label: str, is_default: bool) -> str:
        display = label.removesuffix(" Code")
        return f"{display} (Default)" if is_default else display

    _ACTIVE_STATUSES = frozenset({TaskStatus.IN_PROGRESS, TaskStatus.REVIEW})

    def _notify_if_backend_changed(self, result: TaskUpdateDict) -> None:
        """Show an informational toast when backend is changed on an active task."""
        if self.is_create or not self._task_model:
            return
        if self._task_model.status not in self._ACTIVE_STATUSES:
            return
        new_backend = result.get("agent_backend")
        old_backend = self._task_model.agent_backend
        if new_backend != old_backend:
            self.app.notify(
                "Agent backend updated — change will take effect on the next run",
                severity="information",
            )

    def _validate_and_build_result(self) -> TaskUpdateDict | None:
        """Validate form and build result model. Returns None if validation fails."""
        title_input = self.query_one("#title-input", Input)
        description_input = self.query_one("#description-input", TextArea)
        priority_select: Select[int] = self.query_one("#priority-select", Select)

        title = title_input.value.strip()
        if not title:
            self.notify("Title is required", severity="error")
            title_input.focus()
            return None

        description = description_input.text

        priority_value = priority_select.value
        if priority_value is Select.BLANK:
            self.notify("Priority is required", severity="error")
            priority_select.focus()
            return None
        if not isinstance(priority_value, int):
            self.notify("Priority must be an integer value", severity="error")
            priority_select.focus()
            return None
        priority = TaskPriority(priority_value)

        type_select: Select[str] = self.query_one("#type-select", Select)
        type_value = type_select.value
        if type_value is Select.BLANK:
            task_type = TaskType.PAIR
        else:
            if not isinstance(type_value, str):
                self.notify("Task type must be a string value", severity="error")
                type_select.focus()
                return None
            task_type = TaskType(type_value)

        agent_backend_select: Select[str] = self.query_one("#agent-backend-select", Select)
        agent_backend_value = agent_backend_select.value
        if agent_backend_value is Select.BLANK:
            agent_backend = self._get_default_agent_key()
        else:
            agent_backend = str(agent_backend_value)

        pair_terminal_select: Select[str] = self.query_one("#pair-terminal-backend-select", Select)
        pair_terminal_value = pair_terminal_select.value
        if pair_terminal_value is Select.BLANK:
            terminal_backend = self._get_default_pair_terminal_backend()
        else:
            selected_backend = str(pair_terminal_value)
            terminal_backend = (
                selected_backend
                if selected_backend in VALID_PAIR_BACKENDS
                else _DEFAULT_PAIR_TERMINAL_BACKEND
            )
        if task_type == TaskType.AUTO:
            terminal_backend = None

        acceptance_criteria = self.query_one("#ac-input", AcceptanceCriteriaArea).get_criteria()

        base_branch_input = self.query_one("#base-branch-input", BaseBranchInput)
        base_branch = base_branch_input.value.strip() or None

        if self.is_create:
            status_select: Select[str] = self.query_one("#status-select", Select)
            status_value = status_select.value
            if status_value is Select.BLANK:
                self.notify("Status is required", severity="error")
                status_select.focus()
                return None
            if not isinstance(status_value, str):
                self.notify("Status must be a string value", severity="error")
                status_select.focus()
                return None
            status = TaskStatus(status_value)
            result: TaskUpdateDict = {
                "title": title,
                "description": description,
                "priority": priority,
                "task_type": task_type,
                "status": status,
                "agent_backend": agent_backend or None,
                "terminal_backend": terminal_backend,
                "acceptance_criteria": acceptance_criteria,
                "base_branch": base_branch,
            }
            return result

        return {
            "title": title,
            "description": description,
            "priority": priority,
            "task_type": task_type,
            "agent_backend": agent_backend or None,
            "terminal_backend": terminal_backend,
            "acceptance_criteria": acceptance_criteria,
            "base_branch": base_branch,
        }
