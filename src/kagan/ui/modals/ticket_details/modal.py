"""Unified ticket modal for viewing, editing, and creating tickets."""

from __future__ import annotations

import contextlib
from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Input, Label, Rule, Select, Static, TextArea

from kagan.constants import PRIORITY_LABELS
from kagan.database.models import (
    TicketCreate,
    TicketPriority,
    TicketStatus,
    TicketType,
    TicketUpdate,
)
from kagan.keybindings import TICKET_DETAILS_BINDINGS, to_textual_bindings
from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.modals.ticket_details import form
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.database.models import Ticket


class TicketDetailsModal(ModalScreen[ModalAction | TicketCreate | TicketUpdate | None]):
    """Unified modal for viewing, editing, and creating tickets."""

    editing = reactive(False)

    BINDINGS = to_textual_bindings(TICKET_DETAILS_BINDINGS)

    def __init__(
        self, ticket: Ticket | None = None, *, start_editing: bool = False, **kwargs
    ) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket
        self.is_create = ticket is None
        # Check if ticket is in Done status (normalize string/enum)
        self._is_done = False
        if ticket is not None:
            status = ticket.status
            if isinstance(status, str):
                status = TicketStatus(status)
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
            with contextlib.suppress(NoMatches):
                self.query_one("#title-input", Input).focus()

    def compose(self) -> ComposeResult:
        with Vertical(id="ticket-details-container"):
            yield Label(
                form.get_modal_title(self.ticket, self.editing, self.is_create),
                classes="modal-title",
                id="modal-title-label",
            )
            yield Rule(line_style="heavy")

            with Horizontal(classes="badge-row view-only", id="badge-row"):
                yield Label(
                    form.get_priority_label(self.ticket),
                    classes=f"badge {form.get_priority_class(self.ticket)}",
                    id="priority-badge",
                )
                yield Label(
                    form.get_type_label(self.ticket),
                    classes="badge badge-type",
                    id="type-badge",
                )
                yield Label(
                    form.format_status(self.ticket.status if self.ticket else TicketStatus.BACKLOG),
                    classes="badge badge-status",
                    id="status-badge",
                )
                if self.ticket and self.ticket.agent_backend:
                    yield Label(
                        self.ticket.agent_backend,
                        classes="badge badge-agent",
                        id="agent-badge",
                    )

            with Horizontal(classes="field-row edit-fields", id="edit-fields-row"):
                with Vertical(classes="form-field field-third"):
                    yield Label("Priority:", classes="form-label")
                    current_priority = (
                        self.ticket.priority if self.ticket else TicketPriority.MEDIUM
                    )
                    if isinstance(current_priority, int):
                        current_priority = TicketPriority(current_priority)
                    yield Select(
                        options=[(label, p.value) for p, label in PRIORITY_LABELS.items()],
                        value=current_priority.value,
                        id="priority-select",
                    )

                with Vertical(classes="form-field field-third"):
                    yield Label("Type:", classes="form-label")
                    current_type = self.ticket.ticket_type if self.ticket else TicketType.PAIR
                    if isinstance(current_type, str):
                        current_type = TicketType(current_type)
                    # Disable type selector when editing existing ticket
                    is_editing = self.ticket is not None
                    yield Select(
                        options=[
                            ("Pair (tmux)", TicketType.PAIR.value),
                            ("Auto (ACP)", TicketType.AUTO.value),
                        ],
                        value=current_type.value,
                        id="type-select",
                        disabled=is_editing,
                    )

                with Vertical(classes="form-field field-third"):
                    yield Label("Agent:", classes="form-label")
                    agent_options = form.build_agent_options(self.app)
                    current_backend = self.ticket.agent_backend if self.ticket else ""
                    yield Select(
                        options=agent_options,
                        value=current_backend or "",
                        id="agent-backend-select",
                        allow_blank=True,
                    )

            with Vertical(classes="form-field edit-fields", id="status-field"):
                yield Label("Status:", classes="form-label")
                yield Select(
                    options=[
                        ("Backlog", TicketStatus.BACKLOG.value),
                        ("In Progress", TicketStatus.IN_PROGRESS.value),
                        ("Review", TicketStatus.REVIEW.value),
                        ("Done", TicketStatus.DONE.value),
                    ],
                    value=TicketStatus.BACKLOG.value,
                    id="status-select",
                )

            yield Rule()

            yield Label("Title", classes="section-title view-only", id="title-section-label")
            yield Static(
                self.ticket.title if self.ticket else "",
                classes="ticket-title view-only",
                id="title-display",
            )
            with Vertical(classes="form-field edit-fields", id="title-field"):
                yield Input(
                    value=self.ticket.title if self.ticket else "",
                    placeholder="Enter ticket title...",
                    id="title-input",
                )

            yield Rule()

            with Horizontal(classes="description-header"):
                yield Label("Description", classes="section-title")
                yield Static("", classes="header-spacer")
                expand_text = "[f] Expand" if not self.editing else "[F5] Full Editor"
                yield Static(expand_text, classes="expand-hint", id="expand-btn")

            description = (self.ticket.description if self.ticket else "") or "(No description)"
            yield Static(
                description, classes="ticket-description view-only", id="description-content"
            )

            with Vertical(classes="form-field edit-fields", id="description-field"):
                yield TextArea(
                    self.ticket.description if self.ticket else "",
                    id="description-input",
                    show_line_numbers=True,
                )

            # Acceptance criteria section
            if self.ticket and self.ticket.acceptance_criteria:
                with Vertical(classes="acceptance-criteria-section view-only", id="ac-section"):
                    yield Label("Acceptance Criteria", classes="section-title")
                    for criterion in self.ticket.acceptance_criteria:
                        yield Static(f"  - {criterion}", classes="ac-item")

            with Vertical(classes="form-field edit-fields", id="ac-field"):
                yield Label("Acceptance Criteria (one per line):", classes="form-label")
                ac_text = "\n".join(self.ticket.acceptance_criteria) if self.ticket else ""
                yield TextArea(ac_text, id="ac-input")

            if form.has_review_data(self.ticket):
                with Vertical(classes="review-results-section view-only", id="review-section"):
                    yield Label("Review Results", classes="section-title")
                    with Horizontal(classes="review-status-row"):
                        yield Label(
                            form.format_checks_badge(self.ticket),
                            classes=f"badge {form.get_checks_class(self.ticket)}",
                            id="checks-badge",
                        )
                    if self.ticket and self.ticket.review_summary:
                        yield Static(
                            self.ticket.review_summary,
                            classes="review-summary-text",
                            id="review-summary-display",
                        )
                yield Rule()

            with Horizontal(classes="meta-row", id="meta-row"):
                if self.ticket:
                    created = f"Created: {self.ticket.created_at:%Y-%m-%d %H:%M}"
                    updated = f"Updated: {self.ticket.updated_at:%Y-%m-%d %H:%M}"
                    yield Label(created, classes="ticket-meta")
                    yield Static("  |  ", classes="meta-separator")
                    yield Label(updated, classes="ticket-meta")

            yield Rule()

            with Horizontal(classes="button-row view-only", id="view-buttons"):
                yield Button("[Esc] Close", id="close-btn")
                yield Button("[e] Edit", id="edit-btn", disabled=self._is_done)
                yield Button("[d] Delete", variant="error", id="delete-btn")

            with Horizontal(classes="button-row edit-fields", id="edit-buttons"):
                yield Button("[Ctrl+S] Save", variant="primary", id="save-btn")
                yield Button("[Esc] Cancel", id="cancel-btn")

        yield Footer()

    def watch_editing(self, editing: bool) -> None:
        self.set_class(editing, "editing")

        with contextlib.suppress(NoMatches):
            title_label = self.query_one("#modal-title-label", Label)
            title_label.update(form.get_modal_title(self.ticket, editing, self.is_create))

        with contextlib.suppress(NoMatches):
            expand_btn = self.query_one("#expand-btn", Static)
            expand_btn.update("[F5] Full Editor" if editing else "[f] Expand")

        if editing:
            with contextlib.suppress(NoMatches):
                self.query_one("#title-input", Input).focus()

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
                    form.reset_form_fields(self, self.ticket)
        else:
            self.dismiss(None)

    def action_save(self) -> None:
        if not self.editing:
            return
        result = form.validate_and_build_result(self, self.ticket, self.is_create)
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
        if self.editing:
            description_input = self.query_one("#description-input", TextArea)
            current_text = description_input.text

            def handle_result(result: str | None) -> None:
                if result is not None:
                    description_input.text = result

            modal = DescriptionEditorModal(
                description=current_text, readonly=False, title="Edit Description"
            )
            self.app.push_screen(modal, handle_result)
        else:
            description = self.ticket.description if self.ticket else ""
            modal = DescriptionEditorModal(
                description=description, readonly=True, title="View Description"
            )
            self.app.push_screen(modal)
