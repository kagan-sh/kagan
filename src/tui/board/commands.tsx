import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { openCreateTaskDialog } from "../dialogs/create-task"
import { SETTINGS_ROUTE } from "../types"
import { BOARD_BINDINGS } from "./commands/bindings"
import { createBoardActions } from "./commands/context"
import type { createBoardStore } from "./store"
import { footerHints } from "./commands/hints"
import { approve } from "./commands/merge"
import { moveNextWithGates, movePrevWithGates } from "./commands/intake"
import { closeBoard, dismissBoard, openSession, promptDelete, promptFilter } from "./commands/nav"
import { openMenu } from "./commands/menu"
import { openPermissionQueue } from "./commands/permissions"
import { sendBackTask } from "./commands/send-back"
import { retryHelperTask } from "./commands/task-actions"

export type BoardStore = ReturnType<typeof createBoardStore>

export { BOARD_BINDINGS } from "./commands/bindings"
export { footerHints } from "./commands/hints"
export { HelpOverlay } from "./commands/help"

export function createBoardCommands(
  api: TuiPluginApi,
  store: BoardStore,
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void,
) {
  const ctx = createBoardActions(api, store, setHelpOpen)
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
