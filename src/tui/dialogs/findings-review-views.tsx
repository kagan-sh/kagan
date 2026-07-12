/** @jsxImportSource @opentui/solid */
import { TextAttributes } from "@opentui/core"
import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { For, Show } from "solid-js"
import { isResolvedFinding, type Finding } from "../../domain/task/findings"
import { confidenceBar } from "../format"

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

function detailHeader(finding: Finding, index: number, total: number): string {
  return `finding ${index + 1}/${total} · ${finding.category ?? "finding"} · ${finding.severity ?? "unscored"} · confidence ${finding.confidence ?? "?"}/10`
}

function locationMarker(finding?: Finding): string | undefined {
  return finding?.outOfDiff ? "⚠ not found in diff" : undefined
}

function FindingRow(props: { theme: TuiThemeCurrent; finding: Finding; selected: boolean }) {
  const selectedFg = () => props.theme.selectedListItemText
  return (
    <box flexDirection="row" gap={2} backgroundColor={props.selected ? props.theme.primary : undefined}>
      <text
        flexShrink={0}
        wrapMode="none"
        fg={props.selected ? selectedFg() : severityColor(props.theme, props.finding.severity)}
      >
        {severityLabel(props.finding.severity)}
      </text>
      <text flexShrink={0} wrapMode="none" fg={props.selected ? selectedFg() : props.theme.text}>
        {confidenceBar(props.finding.confidence)}
      </text>
      <text flexShrink={0} wrapMode="none" fg={props.selected ? selectedFg() : props.theme.textMuted}>
        {props.finding.category ?? "finding"}
      </text>
      <text
        flexGrow={1}
        flexShrink={1}
        wrapMode="none"
        truncate={true}
        fg={props.selected ? selectedFg() : props.theme.text}
      >
        {props.finding.summary}
      </text>
      <text
        flexShrink={0}
        wrapMode="none"
        fg={
          props.selected ? selectedFg() : isResolvedFinding(props.finding) ? props.theme.success : props.theme.warning
        }
      >
        {rulingLabel(props.finding)}
      </text>
    </box>
  )
}

export function FindingsList(props: { theme: TuiThemeCurrent; findings: Finding[]; index: number }) {
  return (
    <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
      <box flexDirection="column" gap={1}>
        <For each={props.findings}>
          {(finding, i) => <FindingRow theme={props.theme} finding={finding} selected={i() === props.index} />}
        </For>
      </box>
    </scrollbox>
  )
}

export function FindingDetail(props: {
  theme: TuiThemeCurrent
  finding: Finding
  index: number
  total: number
  note: string
  setNote: (value: string) => void
  focus: number
  error?: string
}) {
  return (
    <box flexDirection="column" gap={1}>
      <text fg={props.theme.text}>{detailHeader(props.finding, props.index, props.total)}</text>
      <box flexDirection="column">
        <text fg={props.theme.accent} attributes={TextAttributes.BOLD}>
          Problem
        </text>
        <text fg={props.theme.text}>{props.finding.detail ?? props.finding.summary}</text>
      </box>
      <Show when={props.finding.location}>
        <box flexDirection="column">
          <text fg={props.theme.accent} attributes={TextAttributes.BOLD}>
            Code
          </text>
          <box flexDirection="row" gap={2}>
            <text fg={props.theme.text}>{props.finding.location}</text>
            <Show when={locationMarker(props.finding)}>
              <text fg={props.theme.warning}>{locationMarker(props.finding)}</text>
            </Show>
          </box>
        </box>
      </Show>
      <box flexDirection="column">
        <text fg={props.theme.accent} attributes={TextAttributes.BOLD}>
          {RECALL_PROMPT}
        </text>
        <input focused={props.focus === 0} value={props.note} placeholder={RECALL_PROMPT} onInput={props.setNote} />
      </box>
      <Show when={props.error}>
        <text fg={props.theme.error}>{props.error}</text>
      </Show>
      <box flexDirection="row" gap={2}>
        <For each={RULING_BUTTONS}>
          {(button) => (
            <box
              paddingLeft={1}
              paddingRight={1}
              backgroundColor={props.focus === button.slot ? props.theme.primary : undefined}
            >
              <text fg={props.focus === button.slot ? props.theme.selectedListItemText : props.theme.textMuted}>
                {button.label}
              </text>
            </box>
          )}
        </For>
      </box>
    </box>
  )
}

export function FindingsFooter(props: {
  theme: TuiThemeCurrent
  mode: "list" | "detail"
  clean: boolean
  reason?: string
}) {
  return (
    <box paddingTop={1} flexDirection="row" gap={2}>
      <Show when={props.mode === "list"}>
        <Show when={!props.clean}>
          <box flexDirection="row">
            <text fg={props.theme.text}>enter</text>
            <text fg={props.theme.textMuted}> open</text>
          </box>
        </Show>
        <box flexDirection="row">
          <text fg={props.theme.text}>s</text>
          <text fg={props.theme.textMuted}> send back</text>
        </box>
        <box flexDirection="row">
          <text fg={props.theme.text}>a</text>
          <text fg={props.reason ? props.theme.textMuted : props.theme.success}>
            {" "}
            {props.reason ? "approve" : "approve & merge"}
          </text>
        </box>
        <Show when={props.reason}>
          <text flexShrink={1} wrapMode="none" truncate={true} fg={props.theme.textMuted}>
            ({props.reason})
          </text>
        </Show>
      </Show>
      <Show when={props.mode === "detail"}>
        <box flexDirection="row">
          <text fg={props.theme.text}>tab</text>
          <text fg={props.theme.textMuted}> move</text>
        </box>
        <box flexDirection="row">
          <text fg={props.theme.text}>enter</text>
          <text fg={props.theme.textMuted}> rule</text>
        </box>
        <box flexDirection="row">
          <text fg={props.theme.text}>esc</text>
          <text fg={props.theme.textMuted}> back</text>
        </box>
      </Show>
    </box>
  )
}
