/** @jsxImportSource @opentui/solid */
import type { BoardSession } from "../../types"
import type { BoardCommandContext } from "./types"
import type { MenuAction } from "./hints"
import { menuOptions } from "./hints"
import { moveNextWithGates } from "./intake"
import { openSession, promptDelete } from "./nav"
import { sendBackTask } from "./send-back"
import { archiveSelected, retryHelperTask, viewDetails, viewIntakeNotes } from "./task-actions"

const runMenuAction = async (
  ctx: BoardCommandContext,
  action: MenuAction,
  session: BoardSession,
  moveNext: () => Promise<void>,
  approveTask: () => void,
) => {
  if (action === "view") return viewDetails(ctx, session)
  if (action === "intake") return viewIntakeNotes(ctx, session)
  if (action === "open") return openSession(ctx)
  if (action === "advance") return moveNextWithGates(ctx, approveTask, moveNext)
  if (action === "send_back") return sendBackTask(ctx)
  if (action === "approve") return approveTask()
  if (action === "retry") return retryHelperTask(ctx)
  if (action === "archive") return archiveSelected(ctx)
  promptDelete(ctx)
}

export const openMenu = (ctx: BoardCommandContext, moveNext: () => Promise<void>, approveTask: () => void) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<MenuAction>
      title="Task actions"
      options={menuOptions(session)}
      onSelect={(option) => {
        ctx.api.ui.dialog.clear()
        void runMenuAction(ctx, option.value, session, moveNext, approveTask)
      }}
    />
  ))
}
