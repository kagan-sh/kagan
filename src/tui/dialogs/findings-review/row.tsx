/** @jsxImportSource @opentui/solid */
import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { isResolvedFinding, type Finding } from "../../../domain/task/findings"
import { confidenceBar } from "../../format"

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

export function FindingRow(props: { theme: TuiThemeCurrent; finding: Finding; selected: boolean }) {
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
