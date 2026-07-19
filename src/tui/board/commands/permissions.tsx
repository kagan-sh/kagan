/** @jsxImportSource @opentui/solid */
import { kagan } from "../../../domain/task/metadata"
import { selectSessionOrNotify, type BoardCommandContext } from "./types"

type PendingPermission = { sessionID: string; label: string; description?: string }

function collectPending(ctx: BoardCommandContext): PendingPermission[] {
  const pending: PendingPermission[] = []
  for (const session of ctx.store.sessions()) {
    if (session.parentID) continue
    const view = kagan(session.metadata)
    if (view.boardTask !== true) continue
    const ref = view.taskNumber !== undefined ? `#${view.taskNumber}` : session.id
    const task = session.title ?? session.slug ?? ""
    for (const entry of view.awaitingPermissions ?? []) {
      pending.push({
        sessionID: entry.sessionID,
        label: `${ref} — ${entry.title}`,
        description: task || undefined,
      })
    }
  }
  return pending
}

export const openPermissionQueue = (ctx: BoardCommandContext) => {
  const pending = collectPending(ctx)
  if (pending.length === 0) {
    ctx.store.notify({ variant: "warning", title: "Kagan", message: "No permission requests waiting" })
    return
  }
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<string>
      title={`Permission requests waiting (${pending.length})`}
      placeholder="Select a request to open its session"
      options={pending.map((item) => ({
        title: item.label,
        description: item.description,
        value: item.sessionID,
      }))}
      onSelect={(option) => {
        ctx.api.ui.dialog.clear()
        void selectSessionOrNotify(ctx, option.value)
      }}
    />
  ))
}
