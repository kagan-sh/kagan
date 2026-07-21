import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { openCreateTaskDialog } from "../dialogs/create-task"
import { SETTINGS_ROUTE } from "../types"
import { type BoardStore } from "./store"
import { footerHints } from "./commands/hints"
import { approve } from "./commands/merge"
import { moveNextWithGates, movePrevWithGates } from "./commands/intake"
import { closeBoard, dismissBoard, openSession, promptDelete, promptFilter } from "./commands/nav"
import { openMenu } from "./commands/menu"
import { openPermissionQueue } from "./commands/permissions"
import { sendBackTask } from "./commands/send-back"
import { retryHelperTask } from "./commands/task-actions"
import type { BoardCommandContext } from "./commands/types"

export type { BoardStore } from "./store"
export { footerHints } from "./commands/hints"

export const BOARD_BINDINGS = [
  { key: "j,down", cmd: "kagan.down", desc: "Next row (card or subtask)", short: "down" },
  { key: "k,up", cmd: "kagan.up", desc: "Previous row (card or subtask)", short: "up" },
  { key: "tab", cmd: "kagan.next_root", desc: "Next root task across columns", short: "next task" },
  { key: "shift+tab", cmd: "kagan.prev_root", desc: "Previous root task across columns", short: "prev task" },
  { key: "shift+j", cmd: "kagan.reorder_down", desc: "Move card down in column", short: "reorder down" },
  { key: "shift+k", cmd: "kagan.reorder_up", desc: "Move card up in column", short: "reorder up" },
  { key: "g", cmd: "kagan.first", desc: "Select first row in column", short: "first" },
  { key: "shift+g", cmd: "kagan.last", desc: "Select last row in column", short: "last" },
  { key: "l,right", cmd: "kagan.next_column", desc: "Next column", short: "next col" },
  { key: "h,left", cmd: "kagan.prev_column", desc: "Previous column", short: "prev col" },
  { key: "m", cmd: "kagan.move_next", desc: "Move to next column", short: "move >" },
  { key: "b", cmd: "kagan.move_prev", desc: "Move to previous column", short: "move <" },
  { key: "n", cmd: "kagan.new", desc: "New task", short: "new" },
  { key: "o", cmd: "kagan.open_session", desc: "Open selected session", short: "open" },
  { key: "return", cmd: "kagan.menu", desc: "Open the card action menu", short: "menu" },
  { key: "d", cmd: "kagan.delete", desc: "Delete selected session", short: "delete" },
  { key: "a", cmd: "kagan.approve", desc: "Approve task", short: "approve" },
  { key: "s", cmd: "kagan.send_back", desc: "Send back for another iteration", short: "send back" },
  { key: "r", cmd: "kagan.retry", desc: "Restart intake or review", short: "restart" },
  { key: "p", cmd: "kagan.permissions", desc: "Review waiting permission requests", short: "permissions" },
  { key: "u", cmd: "kagan.update", desc: "Update Kagan", short: "update" },
  { key: "/", cmd: "kagan.filter", desc: "Filter cards", short: "filter" },
  { key: "?", cmd: "kagan.help", desc: "Show help", short: "help" },
  { key: ",", cmd: "kagan.settings", desc: "Open settings", short: "settings" },
  { key: "q", cmd: "kagan.close", desc: "Close Kagan", short: "quit" },
  { key: "escape", cmd: "kagan.dismiss", desc: "Dismiss help or clear filter", short: "dismiss" },
] as const

export function boardBindings(updateAvailable: boolean) {
  return BOARD_BINDINGS.filter((binding) => updateAvailable || binding.cmd !== "kagan.update")
}

export function createBoardCommands(
  api: TuiPluginApi,
  store: BoardStore,
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void,
) {
  const ctx: BoardCommandContext = { api, store, setHelpOpen }
  const moveNext = () => store.moveNext()
  const approveTask = () => approve(ctx, () => void sendBackTask(ctx))
  const command = (name: string, title: string, run: () => void | Promise<void>) => ({
    name,
    title,
    category: "Kagan",
    run,
  })

  return [
    command("kagan.close", "Close Kagan", () => closeBoard(ctx)),
    command("kagan.down", "Next card", store.selectNext),
    command("kagan.up", "Previous card", store.selectPrevious),
    command("kagan.next_root", "Next root task", store.selectNextRoot),
    command("kagan.prev_root", "Previous root task", store.selectPrevRoot),
    command("kagan.next_column", "Next column", store.selectNextColumn),
    command("kagan.prev_column", "Previous column", store.selectPrevColumn),
    command("kagan.reorder_down", "Move card down in column", () => store.reorder(1)),
    command("kagan.reorder_up", "Move card up in column", () => store.reorder(-1)),
    command("kagan.first", "Select first row in column", store.selectFirst),
    command("kagan.last", "Select last row in column", store.selectLast),
    command("kagan.move_next", "Move card to next column", () => moveNextWithGates(ctx, approveTask, moveNext)),
    command("kagan.move_prev", "Move card to previous column", () => movePrevWithGates(ctx, () => sendBackTask(ctx))),
    command("kagan.new", "New task", () => void openCreateTaskDialog(api, store)),
    command("kagan.open_session", "Open selected session", () => openSession(ctx)),
    command("kagan.menu", "Open the card action menu", () => openMenu(ctx, moveNext, approveTask)),
    command("kagan.delete", "Delete selected session", () => promptDelete(ctx)),
    command("kagan.filter", "Filter cards", () => promptFilter(ctx)),
    command("kagan.dismiss", "Dismiss help or clear filter", () => dismissBoard(ctx)),
    command("kagan.approve", "Approve task", approveTask),
    command("kagan.send_back", "Send back for another iteration", () => sendBackTask(ctx)),
    command("kagan.retry", "Restart intake or review", () => retryHelperTask(ctx)),
    command("kagan.permissions", "Review waiting permission requests", () => openPermissionQueue(ctx)),
    command("kagan.settings", "Open settings", () => api.route.navigate(SETTINGS_ROUTE)),
    command("kagan.help", "Show help", () => setHelpOpen((open) => !open)),
  ]
}
