/** @jsxImportSource @opentui/solid */
import { TextAttributes } from "@opentui/core"
import type { TuiPluginApi, TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { createSignal, For, onMount, Show } from "solid-js"
import { resolveSessionFinding } from "../session/tasks"
import {
  isResolvedFinding,
  sortFindingsByConfidence,
  type Finding,
  type FindingResolution,
} from "../../domain/task/findings"
import { approveDenyReason } from "../../domain/task/policy"
import { isSubstantive } from "../../domain/task/intake"
import { kagan } from "../../domain/task/metadata"
import { confidenceBar, formatModeRationale } from "../format"
import { DialogFrame } from "./chrome"
import type { createBoardStore } from "../board/store"
import type { BoardSession } from "../types"
import { useKeyIntercept } from "../renderer"

type BoardStore = ReturnType<typeof createBoardStore>
type Mode = "list" | "detail"

const SLOT_RESOLUTIONS = [undefined, "ignored", "intended", "clarified"] as const satisfies readonly (
  | FindingResolution
  | undefined
)[]

const RECALL_PROMPT = "In one line — what does this change do, and what breaks if this finding is right?"

const RULING_BUTTONS = [
  { slot: 1, label: "⊘ ignore" },
  { slot: 2, label: "✓ intended" },
  { slot: 3, label: "✎ clarify & answer" },
] as const

const SEVERITY_WIDTH = 4

function severityLabel(severity?: Finding["severity"]): string {
  const word = severity === "high" ? "high" : severity === "medium" ? "med" : severity === "low" ? "low" : "—"
  return word.padEnd(SEVERITY_WIDTH)
}

function severityColor(theme: TuiThemeCurrent, severity?: Finding["severity"]) {
  if (severity === "high") return theme.error
  if (severity === "medium") return theme.warning
  return theme.textMuted
}

function rulingLabel(finding: Finding): string {
  if (finding.resolution === "ignored") return `⊘ ignored${finding.note ? ` (${finding.note})` : ""}`
  if (finding.resolution === "clarified") return `✎ clarified${finding.note ? ` (${finding.note})` : ""}`
  if (finding.resolution === "intended") return `✓ intended${finding.note ? ` (${finding.note})` : ""}`
  return "! untriaged"
}

function requiresNote(resolution: FindingResolution, finding: Finding): boolean {
  if (resolution === "ignored" || resolution === "clarified") return true
  return resolution === "intended" && finding.severity === "high"
}

function detailHeader(finding: Finding, index: number, total: number): string {
  return `finding ${index + 1}/${total} · ${finding.category ?? "finding"} · ${finding.severity ?? "unscored"} · confidence ${finding.confidence ?? "?"}/10`
}

function locationMarker(finding?: Finding): string | undefined {
  return finding?.outOfDiff ? "⚠ not found in diff" : undefined
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

  useKeyIntercept(props.api, (key) => {
    if (mode() === "list") {
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
  })

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
          <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
            <box flexDirection="column" gap={1}>
              <For each={findings()}>
                {(finding, i) => {
                  const selected = () => i() === index()
                  const selectedFg = () => theme().selectedListItemText
                  return (
                    <box flexDirection="row" gap={2} backgroundColor={selected() ? theme().primary : undefined}>
                      <text
                        flexShrink={0}
                        wrapMode="none"
                        fg={selected() ? selectedFg() : severityColor(theme(), finding.severity)}
                      >
                        {severityLabel(finding.severity)}
                      </text>
                      <text flexShrink={0} wrapMode="none" fg={selected() ? selectedFg() : theme().text}>
                        {confidenceBar(finding.confidence)}
                      </text>
                      <text flexShrink={0} wrapMode="none" fg={selected() ? selectedFg() : theme().textMuted}>
                        {finding.category ?? "finding"}
                      </text>
                      <text
                        flexGrow={1}
                        flexShrink={1}
                        wrapMode="none"
                        truncate={true}
                        fg={selected() ? selectedFg() : theme().text}
                      >
                        {finding.summary}
                      </text>
                      <text
                        flexShrink={0}
                        wrapMode="none"
                        fg={selected() ? selectedFg() : isResolvedFinding(finding) ? theme().success : theme().warning}
                      >
                        {rulingLabel(finding)}
                      </text>
                    </box>
                  )
                }}
              </For>
            </box>
          </scrollbox>
        </Show>
      </Show>

      <Show when={mode() === "detail" && current()}>
        <box flexDirection="column" gap={1}>
          <text fg={theme().text}>
            {current() ? detailHeader(current() as Finding, index(), findings().length) : ""}
          </text>
          <box flexDirection="column">
            <text fg={theme().accent} attributes={TextAttributes.BOLD}>
              Problem
            </text>
            <text fg={theme().text}>{current()?.detail ?? current()?.summary}</text>
          </box>
          <Show when={current()?.location}>
            <box flexDirection="column">
              <text fg={theme().accent} attributes={TextAttributes.BOLD}>
                Code
              </text>
              <box flexDirection="row" gap={2}>
                <text fg={theme().text}>{current()?.location}</text>
                <Show when={locationMarker(current())}>
                  <text fg={theme().warning}>{locationMarker(current())}</text>
                </Show>
              </box>
            </box>
          </Show>
          <box flexDirection="column">
            <text fg={theme().accent} attributes={TextAttributes.BOLD}>
              {RECALL_PROMPT}
            </text>
            <input
              focused={focus() === 0}
              value={note()}
              placeholder={RECALL_PROMPT}
              onInput={(value) => setNote(value)}
            />
          </box>
          <Show when={error()}>
            <text fg={theme().error}>{error()}</text>
          </Show>
          <box flexDirection="row" gap={2}>
            <For each={RULING_BUTTONS}>
              {(button) => (
                <box
                  paddingLeft={1}
                  paddingRight={1}
                  backgroundColor={focus() === button.slot ? theme().primary : undefined}
                >
                  <text fg={focus() === button.slot ? theme().selectedListItemText : theme().textMuted}>
                    {button.label}
                  </text>
                </box>
              )}
            </For>
          </box>
        </box>
      </Show>

      <box paddingTop={1} flexDirection="row" gap={2}>
        <Show when={mode() === "list"}>
          <Show when={!clean()}>
            <box flexDirection="row">
              <text fg={theme().text}>enter</text>
              <text fg={theme().textMuted}> open</text>
            </box>
          </Show>
          <box flexDirection="row">
            <text fg={theme().text}>s</text>
            <text fg={theme().textMuted}> send back</text>
          </box>
          <box flexDirection="row">
            <text fg={theme().text}>a</text>
            <text fg={reason() ? theme().textMuted : theme().success}> {reason() ? "approve" : "approve & merge"}</text>
          </box>
          <Show when={reason()}>
            <text flexShrink={1} wrapMode="none" truncate={true} fg={theme().textMuted}>
              ({reason()})
            </text>
          </Show>
        </Show>
        <Show when={mode() === "detail"}>
          <box flexDirection="row">
            <text fg={theme().text}>tab</text>
            <text fg={theme().textMuted}> move</text>
          </box>
          <box flexDirection="row">
            <text fg={theme().text}>enter</text>
            <text fg={theme().textMuted}> rule</text>
          </box>
          <box flexDirection="row">
            <text fg={theme().text}>esc</text>
            <text fg={theme().textMuted}> back</text>
          </box>
        </Show>
      </box>
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
