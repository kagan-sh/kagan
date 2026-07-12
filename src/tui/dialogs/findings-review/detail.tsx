/** @jsxImportSource @opentui/solid */
import { TextAttributes } from "@opentui/core"
import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { For, Show } from "solid-js"
import type { Finding } from "../../../domain/task/findings"

const RECALL_PROMPT = "In one line — what does this change do, and what breaks if this finding is right?"

const RULING_BUTTONS = [
  { slot: 1, label: "⊘ ignore" },
  { slot: 2, label: "✓ intended" },
  { slot: 3, label: "✎ clarify & answer" },
] as const

function detailHeader(finding: Finding, index: number, total: number): string {
  return `finding ${index + 1}/${total} · ${finding.category ?? "finding"} · ${finding.severity ?? "unscored"} · confidence ${finding.confidence ?? "?"}/10`
}

function locationMarker(finding?: Finding): string | undefined {
  return finding?.outOfDiff ? "⚠ not found in diff" : undefined
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
