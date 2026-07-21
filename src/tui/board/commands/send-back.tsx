/** @jsxImportSource @opentui/solid */
import { kagan } from "../../../domain/task/metadata"
import { sendBack } from "../../tasks"
import type { BoardSession } from "../../types"
import { selectSessionOrNotify, type BoardCommandContext } from "./types"

type SendBackChoice = "send_back" | "take_over" | "leave"

const doSendBack = async (ctx: BoardCommandContext, session: BoardSession) => {
  try {
    await sendBack(ctx.api, session)
    await ctx.store.refresh()
    ctx.store.notify({ variant: "success", title: "Kagan", message: "Sent back for another iteration" })
  } catch (error) {
    ctx.store.notify({
      variant: "error",
      title: "Kagan",
      message: error instanceof Error ? error.message : String(error),
    })
  }
}

export const sendBackTask = async (ctx: BoardCommandContext) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  if (session.kaganStatus !== "review") {
    ctx.store.notify({ variant: "warning", title: "Kagan", message: "Send back only applies to tasks in review" })
    return
  }
  const reason = ctx.store.moveDenyReason("in_progress", session)
  if (reason) {
    ctx.store.notify({ variant: "warning", title: "Kagan", message: reason })
    return
  }
  const generation = kagan(session.metadata).generation
  if (generation < ctx.store.sendBackStopThreshold) {
    await doSendBack(ctx, session)
    return
  }
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<SendBackChoice>
      title={`This task has already been sent back ${generation} times. Keep iterating?`}
      options={[
        { title: `Send back again (iteration ${generation + 1})`, value: "send_back" },
        { title: "Take it over yourself", value: "take_over" },
        { title: "Leave it in Review", value: "leave" },
      ]}
      onSelect={(option) => {
        ctx.api.ui.dialog.clear()
        if (option.value === "send_back") {
          void doSendBack(ctx, session)
        } else if (option.value === "take_over") {
          void selectSessionOrNotify(ctx, session.id)
        }
      }}
    />
  ))
}
