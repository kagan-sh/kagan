/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { archiveSession, retryHelper } from "../../session/tasks"
import { canRestartHelper } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import { bunGitRunner } from "../../../git/runner"
import { worktreeDiffs } from "../../../git/diffs"
import { buildTaskDetails, openIntakeNotesView, openTaskDetailsView } from "../../dialogs/task-details"
import type { BoardSession } from "../../types"
import type { BoardCommandContext } from "./types"

const taskDetailsDiffs = async (metadata: Record<string, unknown> | undefined): Promise<Array<SnapshotFileDiff>> => {
  const worktree = kagan(metadata).worktree
  if (!worktree) return []
  try {
    return await worktreeDiffs(bunGitRunner(), worktree, kagan(metadata).baseBranch ?? "HEAD")
  } catch {
    return []
  }
}

export const viewDetails = async (ctx: BoardCommandContext, session: BoardSession) => {
  const details = buildTaskDetails(session.metadata ?? {}, await taskDetailsDiffs(session.metadata), session.title)
  const title = details.taskNumber !== undefined ? `#${details.taskNumber} ${session.title}` : session.title
  openTaskDetailsView(ctx.api, details, title)
}

export const viewIntakeNotes = (ctx: BoardCommandContext, session: BoardSession) => {
  const intake = kagan(session.metadata).intake
  if (!intake) {
    ctx.store.notify({ variant: "warning", title: "Kagan", message: "No intake notes yet" })
    return
  }
  const number = kagan(session.metadata).taskNumber
  const title = number !== undefined ? `Intake notes · #${number}` : "Intake notes"
  openIntakeNotesView(ctx.api, intake, title)
}

export const archiveSelected = async (ctx: BoardCommandContext) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  try {
    await archiveSession(ctx.api, session.id)
    await ctx.store.refresh()
    ctx.store.notify({ variant: "success", title: "Kagan", message: "Archived — still available in the session list" })
  } catch (error) {
    ctx.store.notify({
      variant: "error",
      title: "Kagan",
      message: error instanceof Error ? error.message : String(error),
    })
  }
}

export const retryHelperTask = async (ctx: BoardCommandContext) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  const status = session.kaganStatus
  if (!canRestartHelper(status, session.metadata)) {
    ctx.store.notify({ variant: "warning", title: "Kagan", message: "Nothing to restart" })
    return
  }
  try {
    await retryHelper(ctx.api, session.id, session, status)
    await ctx.store.refresh()
    ctx.store.notify({
      variant: "success",
      title: "Kagan",
      message: status === "backlog" ? "Restarting intake" : "Restarting review",
    })
  } catch (error) {
    ctx.store.notify({
      variant: "error",
      title: "Kagan",
      message: error instanceof Error ? error.message : String(error),
    })
  }
}
