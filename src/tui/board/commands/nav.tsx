/** @jsxImportSource @opentui/solid */
import { selectSessionOrNotify, type BoardCommandContext } from "./types"

export const openSession = async (ctx: BoardCommandContext) => {
  const id = ctx.store.selected()
  if (!id) return
  await selectSessionOrNotify(ctx, id)
}

export const closeBoard = (ctx: BoardCommandContext) => {
  ctx.setHelpOpen((open) => {
    if (!open) ctx.api.route.navigate("home")
    return false
  })
}

export const dismissBoard = (ctx: BoardCommandContext) => {
  ctx.setHelpOpen((open) => {
    if (open) return false
    if (ctx.store.filter() !== "") ctx.store.setFilter("")
    return open
  })
}

export const promptFilter = (ctx: BoardCommandContext) => {
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

export const promptDelete = (ctx: BoardCommandContext) => {
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
