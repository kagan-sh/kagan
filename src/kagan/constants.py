from kagan.core.models.enums import TaskPriority, TaskStatus
from kagan.limits import (
    AGENT_TIMEOUT,
    AGENT_TIMEOUT_LONG,
    DEBUG_BUILD,
    MESSAGE_BUFFER,
    RESPONSE_BUFFER,
    SCRATCHPAD_LIMIT,
    SHUTDOWN_TIMEOUT,
    SUBPROCESS_LIMIT,
)
from kagan.paths import get_config_path, get_database_path

CARD_TITLE_LINE_WIDTH = 28
CARD_DESC_MAX_LENGTH = 28
CARD_REVIEW_MAX_LENGTH = 18
CARD_HAT_MAX_LENGTH = 8
CARD_ID_MAX_LENGTH = 4
CARD_BACKEND_MAX_LENGTH = 10


MODAL_TITLE_MAX_LENGTH = 50
APPROVAL_TITLE_MAX_LENGTH = 45
NOTIFICATION_TITLE_MAX_LENGTH = 40
PLANNER_TITLE_MAX_LENGTH = 30


DIFF_MAX_LENGTH = 10000


MIN_SCREEN_WIDTH = 80
MIN_SCREEN_HEIGHT = 20


# Note: MCP config files (.mcp.json for Claude, opencode.json for OpenCode)


KAGAN_GENERATED_PATTERNS = (
    ".mcp.json",
    "opencode.json",
    "kagan*.mcp.json",
    "*kagan.json",
    ".vscode/mcp.json",
    ".cursor/mcp.json",
    ".kagan/",
    ".gitignore",
)

DEFAULT_DB_PATH = str(get_database_path())
DEFAULT_CONFIG_PATH = str(get_config_path())

COLUMN_ORDER = [
    TaskStatus.BACKLOG,
    TaskStatus.IN_PROGRESS,
    TaskStatus.REVIEW,
    TaskStatus.DONE,
]

STATUS_LABELS = {
    TaskStatus.BACKLOG: "BACKLOG",
    TaskStatus.IN_PROGRESS: "IN PROGRESS",
    TaskStatus.REVIEW: "REVIEW",
    TaskStatus.DONE: "DONE",
}

PRIORITY_LABELS = {
    TaskPriority.LOW: "Low",
    TaskPriority.MEDIUM: "Medium",
    TaskPriority.HIGH: "High",
}


KAGAN_LOGO = """\
ᘚᘛ  ██╗  ██╗ █████╗  ██████╗  █████╗ ███╗   ██╗  ᘚᘛ
ᘚᘛ  ██║ ██╔╝██╔══██╗██╔════╝ ██╔══██╗████╗  ██║  ᘚᘛ
ᘚᘛ  █████╔╝ ███████║██║  ███╗███████║██╔██╗ ██║  ᘚᘛ
ᘚᘛ  ██╔═██╗ ██╔══██║██║   ██║██╔══██║██║╚██╗██║  ᘚᘛ
ᘚᘛ  ██║  ██╗██║  ██║╚██████╔╝██║  ██║██║ ╚████║  ᘚᘛ
ᘚᘛ  ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝  ᘚᘛ"""


KAGAN_LOGO_SMALL = "ᘚᘛ"


BOX_DRAWING = {
    "THICK_TL": "┏",
    "THICK_TR": "┓",
    "THICK_BL": "┗",
    "THICK_BR": "┛",
    "THICK_H": "━",
    "THICK_V": "┃",
    "THIN_TL": "┌",
    "THIN_TR": "┐",
    "THIN_BL": "└",
    "THIN_BR": "┘",
    "THIN_H": "─",
    "THIN_V": "│",
    "LIGHT_H": "─",
    "HEAVY_H": "═",
    "BULLET": "•",
    "DOT": "∙",
    "ARROW_RIGHT": "→",
    "ARROW_DOWN": "↓",
    "SECTION": "─",
    "DIVIDER_LIGHT": "─",
    "DIVIDER_HEAVY": "═",
}

__all__ = [
    "AGENT_TIMEOUT",
    "AGENT_TIMEOUT_LONG",
    "APPROVAL_TITLE_MAX_LENGTH",
    "BOX_DRAWING",
    "CARD_BACKEND_MAX_LENGTH",
    "CARD_DESC_MAX_LENGTH",
    "CARD_HAT_MAX_LENGTH",
    "CARD_ID_MAX_LENGTH",
    "CARD_REVIEW_MAX_LENGTH",
    "CARD_TITLE_LINE_WIDTH",
    "COLUMN_ORDER",
    "DEBUG_BUILD",
    "DEFAULT_CONFIG_PATH",
    "DEFAULT_DB_PATH",
    "DIFF_MAX_LENGTH",
    "KAGAN_GENERATED_PATTERNS",
    "KAGAN_LOGO",
    "KAGAN_LOGO_SMALL",
    "MESSAGE_BUFFER",
    "MIN_SCREEN_HEIGHT",
    "MIN_SCREEN_WIDTH",
    "MODAL_TITLE_MAX_LENGTH",
    "NOTIFICATION_TITLE_MAX_LENGTH",
    "PLANNER_TITLE_MAX_LENGTH",
    "PRIORITY_LABELS",
    "RESPONSE_BUFFER",
    "SCRATCHPAD_LIMIT",
    "SHUTDOWN_TIMEOUT",
    "STATUS_LABELS",
    "SUBPROCESS_LIMIT",
]
