from kagan.database.models import TicketPriority, TicketStatus

DEFAULT_DB_PATH = ".kagan/state.db"
DEFAULT_CONFIG_PATH = ".kagan/config.toml"
DEFAULT_LOCK_PATH = ".kagan/kagan.lock"

COLUMN_ORDER = [
    TicketStatus.BACKLOG,
    TicketStatus.IN_PROGRESS,
    TicketStatus.REVIEW,
    TicketStatus.DONE,
]

STATUS_LABELS = {
    TicketStatus.BACKLOG: "BACKLOG",
    TicketStatus.IN_PROGRESS: "IN PROGRESS",
    TicketStatus.REVIEW: "REVIEW",
    TicketStatus.DONE: "DONE",
}

PRIORITY_LABELS = {
    TicketPriority.LOW: "Low",
    TicketPriority.MEDIUM: "Medium",
    TicketPriority.HIGH: "High",
}
