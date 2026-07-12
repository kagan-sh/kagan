/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { For, Show } from "solid-js"
import { approveSession, archiveSession, resolveSessionIntakeDecision, retryHelper } from "../session/tasks"
import {
  approveDenyReason,
  canRestartHelper,
  intakeReady,
  pendingRequiredIntakeDecisions,
} from "../../domain/task/policy"
import { isSubstantive } from "../../domain/task/intake"
import { kagan } from "../../domain/task/metadata"
import { mergeTask, sendBack } from "../tasks"
import { formatModeRationale } from "../format"
import { baseBranchFreshness, bunGitRunner, currentBranch, listLocalBranches } from "../../git/runner"
import { worktreeDiffs } from "../../git/diffs"
import { openCreateTaskDialog } from "../dialogs/create-task"
import { openFindingsReviewDialog } from "../dialogs/findings-review"
import { buildTaskDetails, openTaskDetailsView } from "../dialogs/task-details"
import type { createBoardStore } from "./store"
import { SETTINGS_ROUTE, type BoardSession } from "../types"

export type BoardStore = ReturnType<typeof createBoardStore>

export const BOARD_BINDINGS = [
  { key: "j,down", cmd: "kagan.down", desc: "Next row (card or subtask)", short: "down" },
  { key: "k,up", cmd: "kagan.up", desc: "Previous row (card or subtask)", short: "up" },
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
  { key: "/", cmd: "kagan.filter", desc: "Filter cards", short: "filter" },
  { key: "?", cmd: "kagan.help", desc: "Show help", short: "help" },
  { key: ",", cmd: "kagan.settings", desc: "Open settings", short: "settings" },
  { key: "q", cmd: "kagan.close", desc: "Close Kagan", short: "quit" },
  { key: "escape", cmd: "kagan.dismiss", desc: "Dismiss help or clear filter", short: "dismiss" },
] as const

type MergeChoice = "current" | "another" | "none"

type MergeDialogHandlers = {
  runMerge: (session: BoardSession, targetBranch: string) => Promise<void>
  promptAnotherBranch: (session: BoardSession) => Promise<void>
  finalizeApprove: (session: BoardSession, mergeMessage?: string) => Promise<void>
}

function mergeChoiceOptions(current: string | undefined, squash: boolean): { title: string; value: MergeChoice }[] {
  const verb = squash ? "Squash-merge" : "Merge"
  const options: { title: string; value: MergeChoice }[] = []
  if (current) options.push({ title: `${verb} into ${current}`, value: "current" })
  options.push({ title: `${verb} into another branch…`, value: "another" })
  options.push({ title: "No action", value: "none" })
  return options
}

function openMergeDialog(
  api: TuiPluginApi,
  store: BoardStore,
  session: BoardSession,
  freshness: { ahead: number },
  handlers: MergeDialogHandlers,
): void {
  const current = api.state.vcs?.branch
  const options = mergeChoiceOptions(current, store.squashMerge)
  const baseBranch = kagan(session.metadata).baseBranch
  const title =
    freshness.ahead > 0 && baseBranch
      ? `Approve — merge the task branch? ${baseBranch} is ${freshness.ahead} commit(s) ahead — the reviewed diff may be stale`
      : "Approve — merge the task branch?"
  api.ui.dialog.replace(() => (
    <api.ui.DialogSelect<MergeChoice>
      title={title}
      options={options}
      onSelect={(option) => {
        if (option.value === "current" && current) {
          api.ui.dialog.clear()
          return handlers.runMerge(session, current)
        }
        if (option.value === "another") {
          return handlers.promptAnotherBranch(session)
        }
        api.ui.dialog.clear()
        return handlers.finalizeApprove(session)
      }}
    />
  ))
}

type FooterHint = { key: string; label: string }

export function footerHints(selected: BoardSession | undefined, hasFilter: boolean): FooterHint[] {
  const hints: FooterHint[] = [
    { key: "j/k/h/l", label: "navigate" },
    { key: "enter", label: "menu" },
    { key: "n", label: "new" },
  ]
  if (selected) {
    if (selected.kaganStatus === "review") {
      hints.push({ key: "a", label: "approve" }, { key: "s", label: "send back" })
    }
    const restartable = canRestartHelper(selected.kaganStatus, selected.metadata)
    if (restartable) {
      hints.push({
        key: "r",
        label: selected.kaganStatus === "backlog" ? "restart intake" : "restart review",
      })
    }
  }
  hints.push({ key: "/", label: "filter" })
  if (hasFilter) hints.push({ key: "esc", label: "clears it" })
  hints.push({ key: ",", label: "settings" }, { key: "?", label: "help" }, { key: "q", label: "quit" })
  return hints
}

type MenuAction = "view" | "open" | "advance" | "send_back" | "approve" | "retry" | "archive" | "delete"

type MenuOption = { title: string; value: MenuAction }

// Mirrors the direct shortcuts (o/m/s/a/r/d) in each title so the menu teaches the fast path;
// options with no dedicated key (view/archive) stay plain.
function menuOptions(session: BoardSession): MenuOption[] {
  const status = session.kaganStatus
  const options: MenuOption[] = [
    { title: "View details", value: "view" },
    { title: "Open session — o", value: "open" },
  ]
  if (status !== "done") options.push({ title: "Advance — m", value: "advance" })
  if (status === "review") {
    options.push({ title: "Send back — s", value: "send_back" }, { title: "Approve — a", value: "approve" })
  }
  const restartable = canRestartHelper(status, session.metadata)
  if (restartable) {
    options.push({
      title: status === "backlog" ? "Restart intake — r" : "Restart review — r",
      value: "retry",
    })
  }
  if (status === "done") options.push({ title: "Archive", value: "archive" })
  options.push({ title: "Delete — d", value: "delete" })
  return options
}

export function HelpOverlay(props: { api: TuiPluginApi; visible: () => boolean }) {
  const theme = () => props.api.theme.current
  const bindings = BOARD_BINDINGS.filter((binding) => binding.cmd !== "kagan.close")

  return (
    <Show when={props.visible()}>
      <box
        position="absolute"
        top={3}
        left={0}
        bottom={2}
        right={0}
        padding={1}
        zIndex={1}
        backgroundColor={theme().backgroundPanel}
      >
        <box flexDirection="row" justifyContent="space-between" flexShrink={0}>
          <text fg={theme().text} attributes={TextAttributes.BOLD}>
            Help
          </text>
          <text fg={theme().textMuted}>esc</text>
        </box>
        <text flexShrink={0} fg={theme().textMuted}>
          New here? Run /kagan-tutorial for the tour.
        </text>
        <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
          <box flexDirection="column">
            <For each={bindings}>
              {(binding) => (
                <box flexDirection="row" gap={2}>
                  <text flexShrink={0} wrapMode="none" fg={theme().textMuted}>
                    {binding.key}
                  </text>
                  <text flexGrow={1} wrapMode="none" truncate={true} fg={theme().text}>
                    {binding.desc}
                  </text>
                </box>
              )}
            </For>
          </box>
        </scrollbox>
      </box>
    </Show>
  )
}

type BoardActions = {
  api: TuiPluginApi
  store: BoardStore
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void
  notifyError: (message: string) => void
  notifyWarning: (message: string) => void
  notifyErrorFrom: (error: unknown) => void
}

const openSession = async (ctx: BoardActions) => {
  const id = ctx.store.selected()
  if (!id) return
  try {
    await ctx.api.client.tui.selectSession({ sessionID: id }, { throwOnError: true })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

const closeBoard = (ctx: BoardActions) => {
  ctx.setHelpOpen((open) => {
    if (!open) ctx.api.route.navigate("home")
    return false
  })
}

const dismissBoard = (ctx: BoardActions) => {
  ctx.setHelpOpen((open) => {
    if (open) return false
    if (ctx.store.filter() !== "") ctx.store.setFilter("")
    return open
  })
}

const promptFilter = (ctx: BoardActions) => {
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

const promptDelete = (ctx: BoardActions) => {
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

const finalizeApprove = async (ctx: BoardActions, session: BoardSession, mergeMessage?: string) => {
  try {
    await approveSession(ctx.api, session.id, session)
    await ctx.store.refresh()
    await ctx.store.moveTo("done")
    ctx.store.notify({
      variant: "success",
      title: "Kagan",
      message: mergeMessage ? `Task approved — ${mergeMessage}` : "Task approved",
    })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

const runMerge = async (ctx: BoardActions, session: BoardSession, targetBranch: string) => {
  const result = await mergeTask(ctx.api, session, targetBranch, ctx.store.squashMerge)
  if (!result.ok) {
    ctx.notifyError(result.message)
    return
  }
  await finalizeApprove(ctx, session, result.message)
}

const promptAnotherBranch = async (ctx: BoardActions, session: BoardSession) => {
  const runner = bunGitRunner()
  const worktree = kagan(session.metadata).worktree
  const taskBranch = worktree ? await currentBranch(runner, worktree) : undefined
  const branches = (await listLocalBranches(runner, ctx.api.state.path.worktree)).filter(
    (branch) => branch !== taskBranch,
  )
  if (branches.length === 0) {
    ctx.notifyWarning("No other local branches to merge into")
    return
  }
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<string>
      title="Merge into which branch?"
      options={branches.map((branch) => ({ title: branch, value: branch }))}
      onSelect={(option) => {
        ctx.api.ui.dialog.clear()
        void runMerge(ctx, session, option.value)
      }}
    />
  ))
}

const promptMerge = async (ctx: BoardActions, session: BoardSession) => {
  const runner = bunGitRunner()
  const view = kagan(session.metadata)
  const freshness = await baseBranchFreshness(runner, view.worktree, view.baseBranch)
  openMergeDialog(ctx.api, ctx.store, session, freshness, {
    runMerge: (target, branch) => runMerge(ctx, target, branch),
    promptAnotherBranch: (target) => promptAnotherBranch(ctx, target),
    finalizeApprove: (target, message) => finalizeApprove(ctx, target, message),
  })
}

const afterTriage = async (ctx: BoardActions, session: BoardSession) => {
  const reason = approveDenyReason(session.metadata)
  if (reason) {
    ctx.notifyWarning(reason)
    return
  }
  await promptMerge(ctx, session)
}

const approve = (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  if (session.kaganStatus !== "review") {
    ctx.notifyWarning("Approve only applies to tasks in review")
    return
  }
  openFindingsReviewDialog(ctx.api, ctx.store, session, ctx.store.checkCommand, {
    onApprove: (approvedSession) => afterTriage(ctx, approvedSession),
    onSendBack: () => void sendBackTask(ctx),
  })
}

const doSendBack = async (ctx: BoardActions, session: BoardSession) => {
  try {
    await sendBack(ctx.api, session)
    await ctx.store.refresh()
    ctx.store.notify({ variant: "success", title: "Kagan", message: "Sent back for another iteration" })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

type SendBackChoice = "send_back" | "take_over" | "leave"

const sendBackTask = async (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  if (session.kaganStatus !== "review") {
    ctx.notifyWarning("Send back only applies to tasks in review")
    return
  }
  const reason = ctx.store.moveDenyReason("in_progress", session)
  if (reason) {
    ctx.notifyWarning(reason)
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
          void ctx.api.client.tui
            .selectSession({ sessionID: session.id }, { throwOnError: true })
            .catch(ctx.notifyErrorFrom)
        }
      }}
    />
  ))
}

const retryHelperTask = async (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  const status = session.kaganStatus
  if (!canRestartHelper(status, session.metadata)) {
    ctx.notifyWarning("Nothing to restart")
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
    ctx.notifyErrorFrom(error)
  }
}

const promptIntakeDecision = (ctx: BoardActions, session: BoardSession, index = 0) => {
  const pending = pendingRequiredIntakeDecisions(session.metadata)
  const decision = pending[index]
  if (!decision) {
    void moveNextWithGates(ctx)
    return
  }

  const commitResolution = async (resolution: "approved" | "overridden", answer?: string) => {
    ctx.api.ui.dialog.clear()
    try {
      await resolveSessionIntakeDecision(ctx.api, session.id, session, decision.id, resolution, answer)
      await ctx.store.refresh()
      const refreshed = ctx.store.sessions().find((item) => item.id === session.id)
      if (!refreshed) return
      if (index + 1 < pending.length) {
        promptIntakeDecision(ctx, refreshed, index + 1)
        return
      }
      await moveNextWithGates(ctx)
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

const taskDetailsDiffs = async (metadata: Record<string, unknown> | undefined): Promise<Array<SnapshotFileDiff>> => {
  const worktree = kagan(metadata).worktree
  if (!worktree) return []
  try {
    return await worktreeDiffs(bunGitRunner(), worktree, kagan(metadata).baseBranch ?? "HEAD")
  } catch {
    return []
  }
}

const viewDetails = async (ctx: BoardActions, session: BoardSession) => {
  const details = buildTaskDetails(session.metadata ?? {}, await taskDetailsDiffs(session.metadata), session.title)
  const title = details.taskNumber !== undefined ? `#${details.taskNumber} ${session.title}` : session.title
  openTaskDetailsView(ctx.api, details, title)
}

const archiveSelected = async (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  try {
    await archiveSession(ctx.api, session.id)
    await ctx.store.refresh()
    ctx.store.notify({ variant: "success", title: "Kagan", message: "Archived — still available in the session list" })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

const startBacklogTask = (ctx: BoardActions, before: BoardSession) => {
  const mode = kagan(before.metadata).intake?.mode
  if (!mode || mode.recommended === "autonomous") {
    void ctx.store.moveNext()
    return
  }
  const rationale = formatModeRationale(before.metadata, ctx.store.checkCommand) ?? mode.rationale
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogConfirm
      title="This one looks better driven by you"
      message={`${rationale} Start the agent on it anyway?`}
      onConfirm={async () => {
        ctx.api.ui.dialog.clear()
        await ctx.store.moveNext()
      }}
      onCancel={() => ctx.api.ui.dialog.clear()}
    />
  ))
}

const moveNextWithGates = async (ctx: BoardActions) => {
  const before = ctx.store.selectedSession()
  if (before && before.kaganStatus === "backlog" && !intakeReady(before.metadata)) {
    if (pendingRequiredIntakeDecisions(before.metadata).length > 0) {
      promptIntakeDecision(ctx, before)
    } else {
      ctx.notifyWarning("Intake is still being prepared")
    }
    return
  }
  if (before && before.kaganStatus === "review") {
    approve(ctx)
    return
  }
  if (before && before.kaganStatus === "backlog") {
    startBacklogTask(ctx, before)
    return
  }
  await ctx.store.moveNext()
}

const movePrevWithGates = async (ctx: BoardActions) => {
  const before = ctx.store.selectedSession()
  if (before && before.kaganStatus === "review") {
    await sendBackTask(ctx)
    return
  }
  await ctx.store.movePrevious()
}

const runMenuAction = async (ctx: BoardActions, action: MenuAction, session: BoardSession) => {
  if (action === "view") return viewDetails(ctx, session)
  if (action === "open") return openSession(ctx)
  if (action === "advance") return moveNextWithGates(ctx)
  if (action === "send_back") return sendBackTask(ctx)
  if (action === "approve") return approve(ctx)
  if (action === "retry") return retryHelperTask(ctx)
  if (action === "archive") return archiveSelected(ctx)
  promptDelete(ctx)
}

const openMenu = (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<MenuAction>
      title="Task actions"
      options={menuOptions(session)}
      onSelect={(option) => {
        ctx.api.ui.dialog.clear()
        void runMenuAction(ctx, option.value, session)
      }}
    />
  ))
}

export function createBoardCommands(
  api: TuiPluginApi,
  store: BoardStore,
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void,
) {
  const ctx: BoardActions = {
    api,
    store,
    setHelpOpen,
    notifyError: (message) => store.notify({ variant: "error", title: "Kagan", message }),
    notifyWarning: (message) => store.notify({ variant: "warning", title: "Kagan", message }),
    notifyErrorFrom: (error) =>
      store.notify({
        variant: "error",
        title: "Kagan",
        message: error instanceof Error ? error.message : String(error),
      }),
  }
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
    command("kagan.move_next", "Move card to next column", () => moveNextWithGates(ctx)),
    command("kagan.move_prev", "Move card to previous column", () => movePrevWithGates(ctx)),
    command("kagan.new", "New task", () => void openCreateTaskDialog(api, store)),
    command("kagan.open_session", "Open selected session", () => openSession(ctx)),
    command("kagan.menu", "Open the card action menu", () => openMenu(ctx)),
    command("kagan.delete", "Delete selected session", () => promptDelete(ctx)),
    command("kagan.filter", "Filter cards", () => promptFilter(ctx)),
    command("kagan.dismiss", "Dismiss help or clear filter", () => dismissBoard(ctx)),
    command("kagan.approve", "Approve task", () => approve(ctx)),
    command("kagan.send_back", "Send back for another iteration", () => sendBackTask(ctx)),
    command("kagan.retry", "Restart intake or review", () => retryHelperTask(ctx)),
    command("kagan.settings", "Open settings", () => api.route.navigate(SETTINGS_ROUTE)),
    command("kagan.help", "Show help", () => setHelpOpen((open) => !open)),
  ]
}
