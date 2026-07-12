/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createSignal, onMount, Show } from "solid-js"
import { resolveSessionFinding } from "../session/tasks"
import { sortFindingsByConfidence, type Finding, type FindingResolution } from "../../domain/task/findings"
import { approveDenyReason } from "../../domain/task/policy"
import { isSubstantive } from "../../domain/task/intake"
import { kagan } from "../../domain/task/metadata"
import { formatModeRationale } from "../format"
import { DialogFrame } from "./chrome"
import { FindingDetail, FindingsFooter, FindingsList } from "./findings-review-views"
import type { createBoardStore } from "../board/store"
import type { BoardSession } from "../types"
import { useKeyIntercept } from "../renderer"

type BoardStore = ReturnType<typeof createBoardStore>
type Mode = "list" | "detail"

const SLOT_RESOLUTIONS = [undefined, "ignored", "intended", "clarified"] as const satisfies readonly (
  | FindingResolution
  | undefined
)[]

function requiresNote(resolution: FindingResolution, finding: Finding): boolean {
  if (resolution === "ignored" || resolution === "clarified") return true
  return resolution === "intended" && finding.severity === "high"
}

function FindingsReview(props: {
  api: TuiPluginApi
  store: BoardStore
  session: BoardSession
  checkCommand?: string
  onApprove: (session: BoardSession) => void
  onSendBack: () => void
}) {
  const theme = () => props.api.theme.current
  const [session, setSession] = createSignal(props.session)
  const [mode, setMode] = createSignal<Mode>("list")
  const [index, setIndex] = createSignal(0)
  const [focus, setFocus] = createSignal(0)
  const [note, setNote] = createSignal("")
  const [error, setError] = createSignal<string | undefined>()
  const modeText = () => formatModeRationale(session().metadata, props.checkCommand)

  onMount(() => props.api.ui.dialog.setSize("large"))

  const findings = () => sortFindingsByConfidence(kagan(session().metadata).findings ?? [])
  const clean = () => findings().length === 0
  const current = () => findings()[index()]
  const reason = () => approveDenyReason(session().metadata)

  const close = () => props.api.ui.dialog.clear()

  const runSendBack = () => {
    close()
    props.onSendBack()
  }

  const runApprove = () => {
    if (reason()) return
    close()
    props.onApprove(session())
  }

  const openDetail = (i: number) => {
    const finding = findings()[i]
    if (!finding) return
    setIndex(i)
    setNote(finding.note ?? "")
    setError(undefined)
    setFocus(0)
    setMode("detail")
  }

  const commit = async (resolution: FindingResolution) => {
    const finding = current()
    if (!finding) return
    const noteValue = note().trim()
    if (requiresNote(resolution, finding) && !isSubstantive(noteValue)) {
      setError("Add a substantive note to rule this way")
      return
    }
    setError(undefined)
    try {
      await resolveSessionFinding(props.api, session().id, session(), finding.id, resolution, noteValue || undefined)
      await props.store.refresh()
      const refreshed = props.store.sessions().find((item) => item.id === session().id)
      if (refreshed) setSession(refreshed as BoardSession)
      setMode("list")
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleListKey = (key: { name: string }): boolean => {
    if (key.name === "escape") {
      close()
      return true
    }
    if (key.name === "s") {
      runSendBack()
      return true
    }
    if (key.name === "a") {
      runApprove()
      return true
    }
    if (clean()) return true
    if (key.name === "down" || key.name === "j") {
      setIndex((i) => Math.min(i + 1, findings().length - 1))
      return true
    }
    if (key.name === "up" || key.name === "k") {
      setIndex((i) => Math.max(i - 1, 0))
      return true
    }
    if (key.name === "return") {
      openDetail(index())
      return true
    }
    return false
  }

  const handleDetailKey = (key: { name: string; shift?: boolean }): boolean => {
    if (key.name === "escape") {
      setError(undefined)
      setMode("list")
      return true
    }
    const slots = SLOT_RESOLUTIONS.length
    if (key.name === "tab") {
      setFocus((f) => (key.shift ? (f + slots - 1) % slots : (f + 1) % slots))
      return true
    }
    if (key.name === "down") {
      setFocus((f) => (f + 1) % slots)
      return true
    }
    if (key.name === "up") {
      setFocus((f) => (f + slots - 1) % slots)
      return true
    }
    if (key.name === "return" && focus() > 0) {
      const resolution = SLOT_RESOLUTIONS[focus()]
      if (resolution) void commit(resolution)
      return true
    }
    return false
  }

  useKeyIntercept(props.api, (key) => (mode() === "list" ? handleListKey(key) : handleDetailKey(key)))

  const title = () => {
    const number = kagan(session().metadata).taskNumber
    return number !== undefined ? `Approve #${number} — triage findings first` : "Approve — triage findings first"
  }

  return (
    <DialogFrame api={props.api} title={title()}>
      <Show when={modeText()}>
        <text fg={theme().textMuted} wrapMode="word">
          {modeText()}
        </text>
      </Show>

      <Show when={mode() === "list"}>
        <Show when={!clean()} fallback={<text fg={theme().textMuted}>No findings — review is clean.</text>}>
          <FindingsList theme={theme()} findings={findings()} index={index()} />
        </Show>
      </Show>

      <Show when={mode() === "detail" && current()}>
        {(finding) => (
          <FindingDetail
            theme={theme()}
            finding={finding()}
            index={index()}
            total={findings().length}
            note={note()}
            setNote={setNote}
            focus={focus()}
            error={error()}
          />
        )}
      </Show>

      <FindingsFooter theme={theme()} mode={mode()} clean={clean()} reason={reason()} />
    </DialogFrame>
  )
}

export function openFindingsReviewDialog(
  api: TuiPluginApi,
  store: BoardStore,
  session: BoardSession,
  checkCommand: string | undefined,
  callbacks: { onApprove: (session: BoardSession) => void; onSendBack: () => void },
): void {
  api.ui.dialog.replace(() => (
    <FindingsReview api={api} store={store} session={session} checkCommand={checkCommand} {...callbacks} />
  ))
}
