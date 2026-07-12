/** @jsxImportSource @opentui/solid */
import { resolveSessionIntakeDecision } from "../../session/tasks"
import { intakeReady, pendingRequiredIntakeDecisions } from "../../../domain/task/policy"
import { isSubstantive } from "../../../domain/task/intake"
import { kagan } from "../../../domain/task/metadata"
import { formatModeRationale } from "../../format"
import type { BoardSession } from "../../types"
import type { BoardActions } from "./context"

const startBacklogTask = (ctx: BoardActions, before: BoardSession, moveNext: () => Promise<void>) => {
  const mode = kagan(before.metadata).intake?.mode
  if (!mode || mode.recommended === "autonomous") {
    void moveNext()
    return
  }
  const rationale = formatModeRationale(before.metadata, ctx.store.checkCommand) ?? mode.rationale
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogConfirm
      title="This one looks better driven by you"
      message={`${rationale} Start the agent on it anyway?`}
      onConfirm={async () => {
        ctx.api.ui.dialog.clear()
        await moveNext()
      }}
      onCancel={() => ctx.api.ui.dialog.clear()}
    />
  ))
}

const promptIntakeDecision = (ctx: BoardActions, session: BoardSession, moveNext: () => Promise<void>, index = 0) => {
  const pending = pendingRequiredIntakeDecisions(session.metadata)
  const decision = pending[index]
  if (!decision) {
    void moveNext()
    return
  }

  const commitResolution = async (resolution: "approved" | "overridden", answer?: string) => {
    ctx.api.ui.dialog.clear()
    try {
      await resolveSessionIntakeDecision(ctx.api, session.id, session, decision.id, resolution, answer)
      await ctx.store.refresh()
      const refreshed = ctx.store.sessions().find((item) => item.id === session.id)
      if (!refreshed) return
      if (pendingRequiredIntakeDecisions(refreshed.metadata).length > 0) {
        promptIntakeDecision(ctx, refreshed, moveNext)
        return
      }
      await moveNext()
    } catch (error) {
      ctx.notifyErrorFrom(error)
    }
  }

  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<"approved" | "overridden">
      title={`Intake decision (${index + 1}/${pending.length})`}
      options={[
        { title: "Approve assumption", value: "approved", description: decision.assumption },
        { title: "Reject & answer", value: "overridden", description: decision.question },
      ]}
      onSelect={(option) => {
        if (option.value === "overridden") {
          ctx.api.ui.dialog.replace(() => (
            <ctx.api.ui.DialogPrompt
              title="Your answer"
              placeholder="Override the assumption (required)"
              onConfirm={async (answer) => {
                if (!isSubstantive(answer)) {
                  ctx.notifyWarning("Add a substantive answer to override this assumption")
                  return
                }
                await commitResolution("overridden", answer)
              }}
              onCancel={() => ctx.api.ui.dialog.clear()}
            />
          ))
          return
        }
        void commitResolution("approved")
      }}
    />
  ))
}

export const moveNextWithGates = async (ctx: BoardActions, approve: () => void, moveNext: () => Promise<void>) => {
  const before = ctx.store.selectedSession()
  if (before && before.kaganStatus === "backlog" && !intakeReady(before.metadata)) {
    if (pendingRequiredIntakeDecisions(before.metadata).length > 0) {
      promptIntakeDecision(ctx, before, moveNext)
    } else {
      ctx.notifyWarning("Intake is still being prepared")
    }
    return
  }
  if (before && before.kaganStatus === "review") {
    approve()
    return
  }
  if (before && before.kaganStatus === "backlog") {
    startBacklogTask(ctx, before, moveNext)
    return
  }
  await moveNext()
}

export const movePrevWithGates = async (ctx: BoardActions, sendBack: () => Promise<void>) => {
  const before = ctx.store.selectedSession()
  if (before && before.kaganStatus === "review") {
    await sendBack()
    return
  }
  await ctx.store.movePrevious()
}
