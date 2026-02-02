"""Unified ticket modal for viewing, editing, and creating tickets."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual import on
from textual.containers import Horizontal, Vertical
from textual.reactive import reactive
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Input, Label, Rule, Select, Static, TextArea

from kagan.database.models import (
    Ticket,
    TicketPriority,
    TicketStatus,
    TicketType,
)
from kagan.keybindings import TICKET_DETAILS_BINDINGS
from kagan.ui.forms.ticket_form import TicketFormBuilder
from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.utils import coerce_enum, copy_with_notification, safe_query_one
from kagan.ui.widgets.base import (
    AcceptanceCriteriaArea,
    AgentBackendSelect,
    DescriptionArea,
    PrioritySelect,
    StatusSelect,
    TicketTypeSelect,
    TitleInput,
)

if TYPE_CHECKING:
    from textual.app import ComposeResult


# Type alias for update data returned by modal
TicketUpdateDict = dict[str, object]


class TicketDetailsModal(ModalScreen[ModalAction | Ticket | TicketUpdateDict | None]):
    """Unified modal for viewing, editing, and creating tickets."""

    editing = reactive(False)

    BINDINGS = TICKET_DETAILS_BINDINGS

    def __init__(
        self,
        ticket: Ticket | None = None,
        *,
        start_editing: bool = False,
        initial_type: TicketType | None = None,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket
        self.is_create = ticket is None
        self._initial_type = initial_type
        # Check if ticket is in Done status (normalize string/enum)
        self._is_done = False
        if ticket is not None:
            status = coerce_enum(ticket.status, TicketStatus)
            self._is_done = status == TicketStatus.DONE
        # Never allow editing Done tickets
        if self._is_done:
            start_editing = False
        self._initial_editing = self.is_create or start_editing

    def on_mount(self) -> None:
        if self.is_create:
            self.add_class("create-mode")
        self.editing = self._initial_editing
        if self.editing:
            if title_input := safe_query_one(self, "#title-input", Input):
                title_input.focus()

    def compose(self) -> ComposeResult:
        with Vertical(id="ticket-details-container"):
            yield Label(
                self._get_modal_title(),
                classes="modal-title",
                id="modal-title-label",
            )
            yield Rule(line_style="heavy")

            # Badge row (view mode)
            yield from self._compose_badge_row()

            # Edit fields row (priority, type, agent)
            yield from self._compose_edit_fields_row()

            # Status field (create mode only)
            if self.is_create:
                with Vertical(classes="form-field edit-fields", id="status-field"):
                    yield Label("Status:", classes="form-label")
                    yield StatusSelect()

            yield Rule()

            # Title field
            yield from self._compose_title_field()

            yield Rule()

            # Description field
            yield from self._compose_description_field()

            # Acceptance criteria
            yield from self._compose_acceptance_criteria()

            # Review results (view mode)
            yield from self._compose_review_section()

            # Meta info
            yield from self._compose_meta_row()

            yield Rule()

            # Buttons
            yield from self._compose_buttons()

        yield Footer()

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
                self._format_status(self.ticket.status if self.ticket else TicketStatus.BACKLOG),
                classes="badge badge-status",
                id="status-badge",
            )
            if self.ticket and self.ticket.agent_backend:
                yield Label(
                    self.ticket.agent_backend,
                    classes="badge badge-agent",
                    id="agent-badge",
                )

    def _compose_edit_fields_row(self) -> ComposeResult:
        """Compose the edit fields row (priority, type, agent)."""
        current_priority = self.ticket.priority if self.ticket else TicketPriority.MEDIUM
        current_priority = coerce_enum(current_priority, TicketPriority)

        current_type = self._initial_type or (
            self.ticket.ticket_type if self.ticket else TicketType.PAIR
        )
        current_type = coerce_enum(current_type, TicketType)

        with Horizontal(classes="field-row edit-fields", id="edit-fields-row"):
            with Vertical(classes="form-field field-third"):
                yield Label("Priority:", classes="form-label")
                yield PrioritySelect(value=current_priority)

            with Vertical(classes="form-field field-third"):
                yield Label("Type:", classes="form-label")
                # Disable type selector when editing existing ticket
                yield TicketTypeSelect(value=current_type, disabled=self.ticket is not None)

            with Vertical(classes="form-field field-third"):
                yield Label("Agent:", classes="form-label")
                agent_options = self._build_agent_options()
                current_backend = self.ticket.agent_backend if self.ticket else ""
                yield AgentBackendSelect(options=agent_options, value=current_backend or "")

    def _compose_title_field(self) -> ComposeResult:
        """Compose the title field."""
        title = self.ticket.title if self.ticket else ""

        yield Label("Title", classes="section-title view-only", id="title-section-label")
        yield Static(title, classes="ticket-title view-only", id="title-display")

        with Vertical(classes="form-field edit-fields", id="title-field"):
            yield TitleInput(value=title)

    def _compose_description_field(self) -> ComposeResult:
        """Compose the description field."""
        with Horizontal(classes="description-header"):
            yield Label("Description", classes="section-title")
            yield Static("", classes="header-spacer")
            expand_text = "[f] Expand" if not self.editing else "[F5] Full Editor"
            yield Static(expand_text, classes="expand-hint", id="expand-btn")

        description = (self.ticket.description if self.ticket else "") or "(No description)"
        yield Static(description, classes="ticket-description view-only", id="description-content")

        with Vertical(classes="form-field edit-fields", id="description-field"):
            yield DescriptionArea(text=self.ticket.description if self.ticket else "")

    def _compose_acceptance_criteria(self) -> ComposeResult:
        """Compose the acceptance criteria section."""
        # View mode
        if self.ticket and self.ticket.acceptance_criteria:
            with Vertical(classes="acceptance-criteria-section view-only", id="ac-section"):
                yield Label("Acceptance Criteria", classes="section-title")
                for criterion in self.ticket.acceptance_criteria:
                    yield Static(f"  - {criterion}", classes="ac-item")

        # Edit mode
        with Vertical(classes="form-field edit-fields", id="ac-field"):
            yield Label("Acceptance Criteria (one per line):", classes="form-label")
            criteria = self.ticket.acceptance_criteria if self.ticket else []
            yield AcceptanceCriteriaArea(criteria=criteria)

    def _compose_review_section(self) -> ComposeResult:
        """Compose the review results section (view mode only)."""
        if not self._has_review_data():
            return

        with Vertical(classes="review-results-section view-only", id="review-section"):
            yield Label("Review Results", classes="section-title")
            with Horizontal(classes="review-status-row"):
                yield Label(
                    self._format_checks_badge(),
                    classes=f"badge {self._get_checks_class()}",
                    id="checks-badge",
                )
            if self.ticket and self.ticket.review_summary:
                yield Static(
                    self.ticket.review_summary,
                    classes="review-summary-text",
                    id="review-summary-display",
                )
        yield Rule()

    def _compose_meta_row(self) -> ComposeResult:
        """Compose the metadata row."""
        with Horizontal(classes="meta-row", id="meta-row"):
            if self.ticket:
                created = f"Created: {self.ticket.created_at:%Y-%m-%d %H:%M}"
                updated = f"Updated: {self.ticket.updated_at:%Y-%m-%d %H:%M}"
                yield Label(created, classes="ticket-meta")
                yield Static("  |  ", classes="meta-separator")
                yield Label(updated, classes="ticket-meta")

    def _compose_buttons(self) -> ComposeResult:
        """Compose the button rows."""
        with Horizontal(classes="button-row view-only", id="view-buttons"):
            yield Button("[Esc] Close", id="close-btn")
            yield Button("[e] Edit", id="edit-btn", disabled=self._is_done)
            yield Button("[d] Delete", variant="error", id="delete-btn")

        with Horizontal(classes="button-row edit-fields", id="edit-buttons"):
            yield Button("[Ctrl+S] Save", variant="primary", id="save-btn")
            yield Button("[Esc] Cancel", id="cancel-btn")

    def watch_editing(self, editing: bool) -> None:
        self.set_class(editing, "editing")

        if title_label := safe_query_one(self, "#modal-title-label", Label):
            title_label.update(self._get_modal_title())

        if expand_btn := safe_query_one(self, "#expand-btn", Static):
            expand_btn.update("[F5] Full Editor" if editing else "[f] Expand")

        # Refresh footer bindings to show appropriate expand key
        self.refresh_bindings()

        if editing:
            if title_input := safe_query_one(self, "#title-input", Input):
                title_input.focus()

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        """Control which bindings are shown based on editing state.

        Returns True to show/enable, False to hide/disable, None for default.
        """
        if action == "expand_description":
            # Show 'f Expand' only in view mode
            return not self.editing
        if action == "full_editor":
            # Show 'F5 Full Editor' only in edit mode
            return self.editing
        return True

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
            self.app.notify("Done tickets cannot be edited", severity="warning")
            return
        if not self.editing and not self.is_create:
            self.editing = True

    def action_delete(self) -> None:
        if not self.editing and self.ticket:
            self.dismiss(ModalAction.DELETE)

    def action_close_or_cancel(self) -> None:
        if self.editing:
            if self.is_create:
                self.dismiss(None)
            else:
                self.editing = False
                if self.ticket:
                    TicketFormBuilder.reset_form_to_ticket(self, self.ticket)
        else:
            self.dismiss(None)

    def action_save(self) -> None:
        if not self.editing:
            return
        result = self._validate_and_build_result()
        if result is not None:
            self.dismiss(result)

    def action_copy(self) -> None:
        """Copy ticket details to clipboard."""
        if not self.ticket:
            self.app.notify("No ticket to copy", severity="warning")
            return
        content = f"#{self.ticket.short_id}: {self.ticket.title}"
        if self.ticket.description:
            content += f"\n\n{self.ticket.description}"
        copy_with_notification(self.app, content, "Ticket")

    def action_expand_description(self) -> None:
        """Expand description in read-only view (for view mode)."""
        if self.editing:
            # In edit mode, this action shouldn't be triggered, but handle gracefully
            self.action_full_editor()
            return
        description = self.ticket.description if self.ticket else ""
        modal = DescriptionEditorModal(
            description=description, readonly=True, title="View Description"
        )
        self.app.push_screen(modal)

    def action_full_editor(self) -> None:
        """Open full editor for description (for edit mode)."""
        if not self.editing:
            # In view mode, this action shouldn't be triggered, but handle gracefully
            self.action_expand_description()
            return
        description_input = self.query_one("#description-input", TextArea)
        current_text = description_input.text

        def handle_result(result: str | None) -> None:
            if result is not None:
                description_input.text = result

        modal = DescriptionEditorModal(
            description=current_text, readonly=False, title="Edit Description"
        )
        self.app.push_screen(modal, handle_result)

    # --- Private helper methods ---

    def _get_modal_title(self) -> str:
        """Get the modal title based on current state."""
        if self.is_create:
            return "New Ticket"
        elif self.editing:
            return "Edit Ticket"
        else:
            return "Ticket Details"

    def _get_priority_label(self) -> str:
        """Get the priority label for display."""
        if not self.ticket:
            return "MED"
        return self.ticket.priority.label

    def _get_priority_class(self) -> str:
        """Get the CSS class for priority badge."""
        if not self.ticket:
            return "badge-priority-medium"
        return f"badge-priority-{self.ticket.priority.css_class}"

    def _get_type_label(self) -> str:
        """Get the type label for display."""
        if not self.ticket:
            return "PAIR"
        if self.ticket.ticket_type == TicketType.AUTO:
            return "AUTO"
        return "PAIR"

    def _format_status(self, status: TicketStatus | str) -> str:
        """Format status for display."""
        status = coerce_enum(status, TicketStatus)
        return status.value.replace("_", " ")

    def _has_review_data(self) -> bool:
        """Check if ticket has review data to display."""
        if not self.ticket:
            return False
        return self.ticket.review_summary is not None or self.ticket.checks_passed is not None

    def _format_checks_badge(self) -> str:
        """Format the checks badge text."""
        if not self.ticket or self.ticket.checks_passed is None:
            return "Not Reviewed"
        return "Approved" if self.ticket.checks_passed else "Rejected"

    def _get_checks_class(self) -> str:
        """Get the CSS class for checks badge."""
        if not self.ticket or self.ticket.checks_passed is None:
            return "badge-checks-pending"
        return "badge-checks-passed" if self.ticket.checks_passed else "badge-checks-failed"

    def _build_agent_options(self) -> list[tuple[str, str]]:
        """Build agent backend options from config."""
        options: list[tuple[str, str]] = [("Default", "")]
        kagan_app = getattr(self.app, "kagan_app", None) or self.app
        if hasattr(kagan_app, "config"):
            for name, agent in kagan_app.config.agents.items():
                if agent.active:
                    options.append((agent.name, name))
        return options

    def _parse_acceptance_criteria(self) -> list[str]:
        """Parse acceptance criteria from AcceptanceCriteriaArea."""
        ac_input = self.query_one("#ac-input", AcceptanceCriteriaArea)
        return ac_input.get_criteria()

    def _validate_and_build_result(self) -> Ticket | TicketUpdateDict | None:
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
        priority = TicketPriority(cast("int", priority_value))

        type_select: Select[str] = self.query_one("#type-select", Select)
        type_value = type_select.value
        if type_value is Select.BLANK:
            ticket_type = TicketType.PAIR
        else:
            ticket_type = TicketType(cast("str", type_value))

        agent_backend_select: Select[str] = self.query_one("#agent-backend-select", Select)
        agent_backend_value = agent_backend_select.value
        agent_backend = str(agent_backend_value) if agent_backend_value is not Select.BLANK else ""

        acceptance_criteria = self._parse_acceptance_criteria()

        if self.is_create:
            status_select: Select[str] = self.query_one("#status-select", Select)
            status_value = status_select.value
            if status_value is Select.BLANK:
                self.notify("Status is required", severity="error")
                status_select.focus()
                return None
            status = TicketStatus(cast("str", status_value))
            return Ticket.create(
                title=title,
                description=description,
                priority=priority,
                ticket_type=ticket_type,
                status=status,
                agent_backend=agent_backend or None,
                acceptance_criteria=acceptance_criteria,
            )
        else:
            # Return dict of updates for existing ticket
            return {
                "title": title,
                "description": description,
                "priority": priority,
                "ticket_type": ticket_type,
                "agent_backend": agent_backend or None,
                "acceptance_criteria": acceptance_criteria,
            }
