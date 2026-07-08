/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { For, Show } from "solid-js"
import {
  approveSession,
  archiveSession,
  mergeTask,
  resolveSessionIntakeDecision,
  retryHelper,
  sendBack,
} from "./session-api"
import {
  approveDenyReason,
  canRetryHelper,
  intakeReady,
  isSubstantive,
  kagan,
  pendingRequiredIntakeDecisions,
} from "./task"
import { formatModeRationale } from "./format"
import { baseBranchFreshness, bunGitRunner, currentBranch, listLocalBranches, worktreeDiffs } from "./git"
import { openCreateTaskDialog } from "./create-task"
import { openFindingsReviewDialog } from "./findings-review"
import { isTrustPacket, openTrustPacketView, serializeTrustPacket } from "./trust-packet"
import type { createBoardStore } from "./store"
import type { BoardSession } from "./types"

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
  { key: "r", cmd: "kagan.retry", desc: "Retry a failed intake or review", short: "retry" },
  { key: "/", cmd: "kagan.filter", desc: "Filter cards", short: "filter" },
  { key: "?", cmd: "kagan.help", desc: "Show help", short: "help" },
  { key: ",", cmd: "kagan.settings", desc: "Open settings", short: "settings" },
  { key: "q", cmd: "kagan.close", desc: "Close Kagan", short: "quit" },
  { key: "escape", cmd: "kagan.dismiss", desc: "Dismiss help or clear filter", short: "dismiss" },
] as const

export type MergeChoice = "current" | "another" | "none"

export type MergeDialogHandlers = {
  runMerge: (session: BoardSession, targetBranch: string) => Promise<void>
  promptAnotherBranch: (session: BoardSession) => Promise<void>
  finalizeApprove: (session: BoardSession, mergeMessage?: string) => Promise<void>
}

export function mergeChoiceOptions(
  current: string | undefined,
  squash: boolean,
): { title: string; value: MergeChoice }[] {
  const verb = squash ? "Squash-merge" : "Merge"
  const options: { title: string; value: MergeChoice }[] = []
  if (current) options.push({ title: `${verb} into ${current}`, value: "current" })
  options.push({ title: `${verb} into another branch…`, value: "another" })
  options.push({ title: "No action", value: "none" })
  return options
}

export function openMergeDialog(
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
          void handlers.runMerge(session, current)
          return
        }
        if (option.value === "another") {
          void handlers.promptAnotherBranch(session)
          return
        }
        api.ui.dialog.clear()
        void handlers.finalizeApprove(session)
      }}
    />
  ))
}

export type FooterHint = { key: string; label: string }

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
    const retryable =
      (selected.kaganStatus === "backlog" && canRetryHelper(selected.metadata, "intake")) ||
      (selected.kaganStatus === "review" && canRetryHelper(selected.metadata, "validator"))
    if (retryable) hints.push({ key: "r", label: "retry" })
  }
  hints.push({ key: "/", label: "filter" })
  if (hasFilter) hints.push({ key: "esc", label: "clears it" })
  hints.push({ key: ",", label: "settings" }, { key: "?", label: "help" }, { key: "q", label: "quit" })
  return hints
}

export type MenuAction =
  | "view"
  | "open"
  | "advance"
  | "send_back"
  | "approve"
  | "retry"
  | "export"
  | "import"
  | "archive"
  | "delete"

export type MenuOption = { title: string; value: MenuAction }

// Mirrors the direct shortcuts (o/m/s/a/r/d) in each title so the menu teaches the fast path;
// options with no dedicated key (view/export/import/archive) stay plain.
export function menuOptions(session: BoardSession): MenuOption[] {
  const status = session.kaganStatus
  const options: MenuOption[] = [
    { title: "View details", value: "view" },
    { title: "Open session — o", value: "open" },
  ]
  if (status !== "done") options.push({ title: "Advance — m", value: "advance" })
  if (status === "review") {
    options.push({ title: "Send back — s", value: "send_back" }, { title: "Approve — a", value: "approve" })
  }
  const retryable =
    (status === "backlog" && canRetryHelper(session.metadata, "intake")) ||
    (status === "review" && canRetryHelper(session.metadata, "validator"))
  if (retryable) options.push({ title: "Retry intake/review — r", value: "retry" })
  options.push({ title: "Export trust packet", value: "export" }, { title: "Import trust packet", value: "import" })
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

export function createBoardCommands(
  api: TuiPluginApi,
  store: BoardStore,
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void,
) {
  const notifyError = (message: string) => {
    store.notify({ variant: "error", title: "Kagan", message })
  }
  const notifyWarning = (message: string) => {
    store.notify({ variant: "warning", title: "Kagan", message })
  }
  const notifyErrorFrom = (error: unknown) => {
    notifyError(error instanceof Error ? error.message : String(error))
  }

  const openSession = async () => {
    const id = store.selected()
    if (!id) return
    try {
      await api.client.tui.selectSession({ sessionID: id }, { throwOnError: true })
    } catch (error) {
      notifyErrorFrom(error)
    }
  }

  const closeBoard = () => {
    setHelpOpen((open) => {
      if (!open) api.route.navigate("home")
      return false
    })
  }

  const dismissBoard = () => {
    setHelpOpen((open) => {
      if (open) return false
      if (store.filter() !== "") store.setFilter("")
      return open
    })
  }

  const promptCreate = () => {
    void openCreateTaskDialog(api, store)
  }

  const promptFilter = () => {
    api.ui.dialog.replace(() => (
      <api.ui.DialogPrompt
        title="Filter cards"
        placeholder="Filter by title"
        value={store.filter()}
        onConfirm={(value) => {
          api.ui.dialog.clear()
          store.setFilter(value)
        }}
        onCancel={() => api.ui.dialog.clear()}
      />
    ))
  }

  const showHelp = () => {
    setHelpOpen((open) => !open)
  }

  const promptDelete = () => {
    const id = store.selected()
    if (!id) return
    const session = store.selectedSession()
    const label = session?.title || session?.slug || id
    api.ui.dialog.replace(() => (
      <api.ui.DialogConfirm
        title="Delete session"
        message={`Permanently delete "${label}"? This cannot be undone.`}
        onConfirm={async () => {
          api.ui.dialog.clear()
          await store.deleteSelected()
        }}
        onCancel={() => api.ui.dialog.clear()}
      />
    ))
  }

  const finalizeApprove = async (session: BoardSession, mergeMessage?: string) => {
    try {
      await approveSession(api, session.id, session)
      await store.refresh()
      await store.moveTo("done")
      store.notify({
        variant: "success",
        title: "Kagan",
        message: mergeMessage ? `Task approved — ${mergeMessage}` : "Task approved",
      })
    } catch (error) {
      notifyErrorFrom(error)
    }
  }

  const runMerge = async (session: BoardSession, targetBranch: string) => {
    const result = await mergeTask(api, session, targetBranch, store.squashMerge)
    if (!result.ok) {
      notifyError(result.message)
      return
    }
    await finalizeApprove(session, result.message)
  }

  const promptAnotherBranch = async (session: BoardSession) => {
    const runner = bunGitRunner()
    const worktree = kagan(session.metadata).worktree
    const taskBranch = worktree ? await currentBranch(runner, worktree) : undefined
    const branches = (await listLocalBranches(runner, api.state.path.worktree)).filter(
      (branch) => branch !== taskBranch,
    )
    if (branches.length === 0) {
      notifyWarning("No other local branches to merge into")
      return
    }
    api.ui.dialog.replace(() => (
      <api.ui.DialogSelect<string>
        title="Merge into which branch?"
        options={branches.map((branch) => ({ title: branch, value: branch }))}
        onSelect={(option) => {
          api.ui.dialog.clear()
          void runMerge(session, option.value)
        }}
      />
    ))
  }

  const promptMerge = async (session: BoardSession) => {
    const runner = bunGitRunner()
    const view = kagan(session.metadata)
    const freshness = await baseBranchFreshness(runner, view.worktree, view.baseBranch)
    openMergeDialog(api, store, session, freshness, {
      runMerge,
      promptAnotherBranch,
      finalizeApprove,
    })
  }

  const afterTriage = async (session: BoardSession) => {
    const reason = approveDenyReason(session.metadata)
    if (reason) {
      notifyWarning(reason)
      return
    }
    await promptMerge(session)
  }

  const approve = async () => {
    const session = store.selectedSession()
    if (!session) return
    if (session.kaganStatus !== "review") {
      notifyWarning("Approve only applies to tasks in review")
      return
    }
    openFindingsReviewDialog(api, store, session, store.checkCommand, {
      onApprove: (approvedSession) => afterTriage(approvedSession),
      onSendBack: () => void sendBackTask(),
    })
  }

  const doSendBack = async (session: BoardSession) => {
    try {
      await sendBack(api, session)
      await store.refresh()
      store.notify({ variant: "success", title: "Kagan", message: "Sent back for another iteration" })
    } catch (error) {
      notifyErrorFrom(error)
    }
  }

  type SendBackChoice = "send_back" | "take_over" | "leave"

  const sendBackTask = async () => {
    const session = store.selectedSession()
    if (!session) return
    if (session.kaganStatus !== "review") {
      notifyWarning("Send back only applies to tasks in review")
      return
    }
    const reason = store.moveDenyReason("in_progress", session)
    if (reason) {
      notifyWarning(reason)
      return
    }
    const generation = kagan(session.metadata).generation
    if (generation < store.sendBackStopThreshold) {
      await doSendBack(session)
      return
    }
    api.ui.dialog.replace(() => (
      <api.ui.DialogSelect<SendBackChoice>
        title={`This task has already been sent back ${generation} times. Keep iterating?`}
        options={[
          { title: `Send back again (iteration ${generation + 1})`, value: "send_back" },
          { title: "Take it over yourself", value: "take_over" },
          { title: "Leave it in Review", value: "leave" },
        ]}
        onSelect={(option) => {
          api.ui.dialog.clear()
          if (option.value === "send_back") {
            void doSendBack(session)
          } else if (option.value === "take_over") {
            void api.client.tui.selectSession({ sessionID: session.id }, { throwOnError: true }).catch(notifyErrorFrom)
          }
        }}
      />
    ))
  }

  const retryHelperTask = async () => {
    const session = store.selectedSession()
    if (!session) return
    const status = session.kaganStatus
    const canRetryIntake = status === "backlog" && canRetryHelper(session.metadata, "intake")
    const canRetryValidator = status === "review" && canRetryHelper(session.metadata, "validator")
    if (!canRetryIntake && !canRetryValidator) {
      notifyWarning("Nothing to retry")
      return
    }
    try {
      await retryHelper(api, session.id, session, status)
      await store.refresh()
      store.notify({
        variant: "success",
        title: "Kagan",
        message: canRetryIntake ? "Retrying intake" : "Retrying review",
      })
    } catch (error) {
      notifyErrorFrom(error)
    }
  }

  const promptIntakeDecision = (session: BoardSession, index = 0) => {
    const pending = pendingRequiredIntakeDecisions(session.metadata)
    const decision = pending[index]
    if (!decision) {
      void moveNextWithGates()
      return
    }

    const commitResolution = async (resolution: "approved" | "overridden", answer?: string) => {
      api.ui.dialog.clear()
      try {
        await resolveSessionIntakeDecision(api, session.id, session, decision.id, resolution, answer)
        await store.refresh()
        const refreshed = store.sessions().find((item) => item.id === session.id)
        if (!refreshed) return
        if (index + 1 < pending.length) {
          promptIntakeDecision(refreshed, index + 1)
          return
        }
        await moveNextWithGates()
      } catch (error) {
        notifyErrorFrom(error)
      }
    }

    api.ui.dialog.replace(() => (
      <api.ui.DialogSelect<"approved" | "overridden">
        title={`Intake decision (${index + 1}/${pending.length})`}
        options={[
          { title: "Approve assumption", value: "approved", description: decision.assumption },
          { title: "Reject & answer", value: "overridden", description: decision.question },
        ]}
        onSelect={(option) => {
          if (option.value === "overridden") {
            api.ui.dialog.replace(() => (
              <api.ui.DialogPrompt
                title="Your answer"
                placeholder="Override the assumption (required)"
                onConfirm={async (answer) => {
                  if (!isSubstantive(answer)) {
                    notifyWarning("Add a substantive answer to override this assumption")
                    return
                  }
                  await commitResolution("overridden", answer)
                }}
                onCancel={() => api.ui.dialog.clear()}
              />
            ))
            return
          }
          void commitResolution("approved")
        }}
      />
    ))
  }

  const packetDiffs = async (metadata: Record<string, unknown> | undefined): Promise<Array<SnapshotFileDiff>> => {
    const worktree = kagan(metadata).worktree
    if (!worktree) return []
    try {
      return await worktreeDiffs(bunGitRunner(), worktree, kagan(metadata).baseBranch ?? "HEAD")
    } catch {
      // Export/peek without diff stats if the worktree cannot be read.
      return []
    }
  }

  const viewDetails = async (session: BoardSession) => {
    const packet = serializeTrustPacket(session.metadata ?? {}, await packetDiffs(session.metadata))
    const title = packet.taskNumber !== undefined ? `#${packet.taskNumber} ${session.title}` : session.title
    openTrustPacketView(api, packet, title)
  }

  const exportPacket = () => {
    const session = store.selectedSession()
    if (!session) {
      notifyWarning("Select a task to export")
      return
    }
    void (async () => {
      const packet = serializeTrustPacket(session.metadata ?? {}, await packetDiffs(session.metadata))
      const defaultName = `kagan-export-${packet.taskNumber ?? session.id.slice(-6)}.json`
      const path = `${api.state.path.worktree}/${defaultName}`
      try {
        await Bun.write(path, JSON.stringify(packet, null, 2))
        store.notify({ variant: "success", title: "Kagan", message: `Exported trust packet to ${path}` })
      } catch (error) {
        notifyErrorFrom(error)
      }
    })()
  }

  const openPacketFile = async (path: string) => {
    try {
      const file = Bun.file(path)
      if (!(await file.exists())) {
        notifyError("File not found")
        return
      }
      const parsed = (await file.json()) as unknown
      if (!isTrustPacket(parsed)) {
        notifyError("Invalid trust packet")
        return
      }
      openTrustPacketView(api, parsed)
    } catch (error) {
      notifyErrorFrom(error)
    }
  }

  const promptImportPath = () => {
    api.ui.dialog.replace(() => (
      <api.ui.DialogPrompt
        title="Import trust packet"
        placeholder="Path to JSON packet"
        value={`${api.state.path.worktree}/`}
        onConfirm={async (path) => {
          api.ui.dialog.clear()
          await openPacketFile(path)
        }}
        onCancel={() => api.ui.dialog.clear()}
      />
    ))
  }

  const importPacket = () => {
    void (async () => {
      const worktree = api.state.path.worktree
      const glob = new Bun.Glob("kagan-export-*.json")
      const matches: Array<{ path: string; mtimeMs: number }> = []
      for await (const path of glob.scan({ cwd: worktree, absolute: true })) {
        matches.push({ path, mtimeMs: (await Bun.file(path).stat()).mtimeMs })
      }
      if (matches.length === 0) {
        promptImportPath()
        return
      }
      matches.sort((left, right) => right.mtimeMs - left.mtimeMs)
      api.ui.dialog.replace(() => (
        <api.ui.DialogSelect<string>
          title="Import trust packet"
          options={matches.map((match) => ({ title: match.path.split("/").pop() ?? match.path, value: match.path }))}
          onSelect={(option) => {
            api.ui.dialog.clear()
            void openPacketFile(option.value)
          }}
        />
      ))
    })()
  }

  const archiveSelected = async () => {
    const session = store.selectedSession()
    if (!session) return
    try {
      await archiveSession(api, session.id)
      await store.refresh()
      store.notify({ variant: "success", title: "Kagan", message: "Archived — still available in the session list" })
    } catch (error) {
      notifyErrorFrom(error)
    }
  }

  const startBacklogTask = (before: BoardSession) => {
    const mode = kagan(before.metadata).intake?.mode
    if (!mode || mode.recommended === "autonomous") {
      void store.moveNext()
      return
    }
    const rationale = formatModeRationale(before.metadata, store.checkCommand) ?? mode.rationale
    api.ui.dialog.replace(() => (
      <api.ui.DialogConfirm
        title="This one looks better driven by you"
        message={`${rationale} Start the agent on it anyway?`}
        onConfirm={async () => {
          api.ui.dialog.clear()
          await store.moveNext()
        }}
        onCancel={() => api.ui.dialog.clear()}
      />
    ))
  }

  const moveNextWithGates = async () => {
    const before = store.selectedSession()
    if (before && before.kaganStatus === "backlog" && !intakeReady(before.metadata)) {
      if (pendingRequiredIntakeDecisions(before.metadata).length > 0) {
        promptIntakeDecision(before)
      } else {
        notifyWarning("Intake is still being prepared")
      }
      return
    }
    if (before && before.kaganStatus === "review") {
      await approve()
      return
    }
    if (before && before.kaganStatus === "backlog") {
      startBacklogTask(before)
      return
    }
    await store.moveNext()
  }

  const movePrevWithGates = async () => {
    const before = store.selectedSession()
    if (before && before.kaganStatus === "review") {
      await sendBackTask()
      return
    }
    await store.movePrevious()
  }

  const runMenuAction = async (action: MenuAction, session: BoardSession) => {
    if (action === "view") return viewDetails(session)
    if (action === "open") return openSession()
    if (action === "advance") return moveNextWithGates()
    if (action === "send_back") return sendBackTask()
    if (action === "approve") return approve()
    if (action === "retry") return retryHelperTask()
    if (action === "export") return exportPacket()
    if (action === "import") return importPacket()
    if (action === "archive") return archiveSelected()
    promptDelete()
  }

  const openMenu = () => {
    const session = store.selectedSession()
    if (!session) return
    api.ui.dialog.replace(() => (
      <api.ui.DialogSelect<MenuAction>
        title="Task actions"
        options={menuOptions(session)}
        onSelect={(option) => {
          api.ui.dialog.clear()
          void runMenuAction(option.value, session)
        }}
      />
    ))
  }

  return [
    {
      name: "kagan.close",
      title: "Close Kagan",
      category: "Kagan",
      run: closeBoard,
    },
    {
      name: "kagan.down",
      title: "Next card",
      category: "Kagan",
      run: store.selectNext,
    },
    {
      name: "kagan.up",
      title: "Previous card",
      category: "Kagan",
      run: store.selectPrevious,
    },
    {
      name: "kagan.next_column",
      title: "Next column",
      category: "Kagan",
      run: store.selectNextColumn,
    },
    {
      name: "kagan.prev_column",
      title: "Previous column",
      category: "Kagan",
      run: store.selectPrevColumn,
    },
    {
      name: "kagan.reorder_down",
      title: "Move card down in column",
      category: "Kagan",
      run: () => store.reorder(1),
    },
    {
      name: "kagan.reorder_up",
      title: "Move card up in column",
      category: "Kagan",
      run: () => store.reorder(-1),
    },
    {
      name: "kagan.first",
      title: "Select first row in column",
      category: "Kagan",
      run: store.selectFirst,
    },
    {
      name: "kagan.last",
      title: "Select last row in column",
      category: "Kagan",
      run: store.selectLast,
    },
    {
      name: "kagan.move_next",
      title: "Move card to next column",
      category: "Kagan",
      run: moveNextWithGates,
    },
    {
      name: "kagan.move_prev",
      title: "Move card to previous column",
      category: "Kagan",
      run: movePrevWithGates,
    },
    {
      name: "kagan.new",
      title: "New task",
      category: "Kagan",
      run: promptCreate,
    },
    {
      name: "kagan.open_session",
      title: "Open selected session",
      category: "Kagan",
      run: openSession,
    },
    {
      name: "kagan.menu",
      title: "Open the card action menu",
      category: "Kagan",
      run: openMenu,
    },
    {
      name: "kagan.delete",
      title: "Delete selected session",
      category: "Kagan",
      run: promptDelete,
    },
    {
      name: "kagan.filter",
      title: "Filter cards",
      category: "Kagan",
      run: promptFilter,
    },
    {
      name: "kagan.dismiss",
      title: "Dismiss help or clear filter",
      category: "Kagan",
      run: dismissBoard,
    },
    {
      name: "kagan.approve",
      title: "Approve task",
      category: "Kagan",
      run: approve,
    },
    {
      name: "kagan.send_back",
      title: "Send back for another iteration",
      category: "Kagan",
      run: sendBackTask,
    },
    {
      name: "kagan.retry",
      title: "Retry a failed intake or review",
      category: "Kagan",
      run: retryHelperTask,
    },
    {
      name: "kagan.export_packet",
      title: "Export trust packet",
      category: "Kagan",
      run: exportPacket,
    },
    {
      name: "kagan.import_packet",
      title: "Import trust packet",
      category: "Kagan",
      run: importPacket,
    },
    {
      name: "kagan.help",
      title: "Show help",
      category: "Kagan",
      run: showHelp,
    },
  ]
}
