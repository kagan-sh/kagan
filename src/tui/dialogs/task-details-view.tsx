/** @jsxImportSource @opentui/solid */
import { TextAttributes } from "@opentui/core"
import { For } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { CheckEvidence, FindingList, IntakeView } from "./task-details-sections"
import type { TaskDetails } from "./task-details"

export function TaskDetailsDialog(props: { api: TuiPluginApi; details: TaskDetails; title: string }) {
  const muted = props.api.theme.current.textMuted
  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} gap={1}>
      <box flexDirection="row" justifyContent="space-between">
        <text attributes={TextAttributes.BOLD}>{props.title}</text>
        <text fg={muted}>esc close</text>
      </box>
      <text>
        {props.details.taskNumber !== undefined ? `#${props.details.taskNumber} ` : ""}
        {props.details.title || props.details.report || "Untitled task"}
      </text>
      <text>
        {props.details.status ? `${props.details.status} · ` : ""}
        Generation {props.details.generation} · {props.details.approved ? "approved" : "not approved"}
        {props.details.baseBranch ? ` · base: ${props.details.baseBranch}` : ""}
      </text>
      {props.details.description ? <text fg={muted}>{props.details.description}</text> : null}
      {props.details.report ? (
        <box flexDirection="column">
          <text attributes={TextAttributes.BOLD}>Report</text>
          <text paddingLeft={2}>{props.details.report}</text>
        </box>
      ) : null}
      {props.details.diffStats.length > 0 ? (
        <box flexDirection="column" gap={1}>
          <text attributes={TextAttributes.BOLD}>Changed files ({props.details.diffStats.length})</text>
          <For each={props.details.diffStats}>
            {(stat) => (
              <text paddingLeft={2}>
                {stat.file} (+{stat.additions}/-{stat.deletions}){stat.status ? ` · ${stat.status}` : ""}
              </text>
            )}
          </For>
        </box>
      ) : null}
      <CheckEvidence api={props.api} label="Setup" result={props.details.setup} />
      <CheckEvidence api={props.api} label="Check" result={props.details.check} />
      <IntakeView intake={props.details.intake} />
      <FindingList api={props.api} title="Findings" findings={props.details.findings} />
      <FindingList api={props.api} title="Prior triage" findings={props.details.priorTriage} />
    </box>
  )
}
