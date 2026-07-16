/** @jsxImportSource @opentui/solid */
import type { BoardActions } from "./context"

export const openSession = async (ctx: BoardActions) => {
  const id = ctx.store.selected()
  if (!id) return
  try {
    await ctx.api.client.tui.selectSession({ sessionID: id }, { throwOnError: true })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

export const closeBoard = (ctx: BoardActions) => {
  ctx.setHelpOpen((open) => {
    if (!open) ctx.api.route.navigate("home")
    return false
  })
}

export const dismissBoard = (ctx: BoardActions) => {
  ctx.setHelpOpen((open) => {
    if (open) return false
    if (ctx.store.filter() !== "") ctx.store.setFilter("")
    return open
  })
}

export const promptFilter = (ctx: BoardActions) => {
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogPrompt
      title="Filter cards"
      placeholder="Filter by title or #N"
      value={ctx.store.filter()}
      onConfirm={(value) => {
        ctx.api.ui.dialog.clear()
        ctx.store.setFilter(value)
      }}
      onCancel={() => ctx.api.ui.dialog.clear()}
    />
  ))
}

export const promptDelete = (ctx: BoardActions) => {
  const id = ctx.store.selected()
  if (!id) return
  const session = ctx.store.selectedSession()
  const label = session?.title || session?.slug || id
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogConfirm
      title="Delete session"
      message={`Permanently delete "${label}"? This cannot be undone.`}
      onConfirm={async () => {
        ctx.api.ui.dialog.clear()
        await ctx.store.deleteSelected()
      }}
      onCancel={() => ctx.api.ui.dialog.clear()}
    />
  ))
}
