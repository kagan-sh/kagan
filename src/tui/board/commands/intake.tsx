/** @jsxImportSource @opentui/solid */
import { resolveSessionIntakeDecision } from "../../session/tasks"
import { intakeReady, pendingRequiredIntakeDecisions } from "../../../domain/task/policy"
import { isSubstantive } from "../../../domain/task/intake"
import { kagan } from "../../../domain/task/metadata"
import { formatModeRationale } from "../../format"
import {
  openIntakeAnswerDialog,
  openIntakeDecisionDialog,
  openIntakeModeConfirmDialog,
} from "../../dialogs/intake-gate"
import type { BoardSession } from "../../types"
import type { BoardCommandContext } from "./types"

const startBacklogTask = (ctx: BoardCommandContext, before: BoardSession, moveNext: () => Promise<void>) => {
  const mode = kagan(before.metadata).intake?.mode
  if (!mode || mode.recommended === "autonomous") {
    void moveNext()
    return
  }
  const rationale = formatModeRationale(before.metadata, ctx.store.checkCommand) ?? mode.rationale
  const recommended = mode.recommended === "manual" ? "manual" : "assisted"
  openIntakeModeConfirmDialog(ctx.api, {
    session: before,
    rationale,
    recommended,
    onConfirm: () => {
      ctx.api.ui.dialog.clear()
      void moveNext()
    },
    onCancel: () => ctx.api.ui.dialog.clear(),
  })
}

const promptIntakeDecision = (
  ctx: BoardCommandContext,
  session: BoardSession,
  moveNext: () => Promise<void>,
  index = 0,
) => {
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
      ctx.store.notify({
        variant: "error",
        title: "Kagan",
        message: error instanceof Error ? error.message : String(error),
      })
    }
  }

  const openDecision = () => {
    openIntakeDecisionDialog(ctx.api, {
      session,
      index,
      total: pending.length,
      decision,
      onCancel: () => ctx.api.ui.dialog.clear(),
      onApprove: () => {
        void commitResolution("approved")
      },
      onReject: () => {
        openIntakeAnswerDialog(ctx.api, {
          session,
          index,
          total: pending.length,
          decision,
          onBack: openDecision,
          onSubmit: (answer) => {
            if (!isSubstantive(answer)) {
              ctx.store.notify({
                variant: "warning",
                title: "Kagan",
                message: "Add a substantive answer to override this assumption",
              })
              return
            }
            void commitResolution("overridden", answer)
          },
        })
      },
    })
  }

  openDecision()
}

export const moveNextWithGates = async (
  ctx: BoardCommandContext,
  approve: () => void,
  moveNext: () => Promise<void>,
) => {
  const before = ctx.store.selectedSession()
  if (before && before.kaganStatus === "backlog" && !intakeReady(before.metadata)) {
    if (pendingRequiredIntakeDecisions(before.metadata).length > 0) {
      promptIntakeDecision(ctx, before, moveNext)
    } else {
      ctx.store.notify({ variant: "warning", title: "Kagan", message: "Intake is still being prepared" })
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

export const movePrevWithGates = async (ctx: BoardCommandContext, sendBack: () => Promise<void>) => {
  const before = ctx.store.selectedSession()
  if (before && before.kaganStatus === "review") {
    await sendBack()
    return
  }
  await ctx.store.movePrevious()
}
