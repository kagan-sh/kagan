/** @jsxImportSource @opentui/solid */
import { TextAttributes } from "@opentui/core"
import { For, onMount } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Intake } from "../../domain/task/intake"
import { useRendererDimensions } from "../renderer"
import { DialogFrame } from "./chrome"
import { CheckEvidence, FindingList, IntakeView } from "./task-details-sections"
import type { TaskDetails } from "./task-details-sections"

/** Host Dialog pads top by height/4; reserve rows for title, summary, footer, gaps. */
export function dialogScrollMaxHeight(terminalHeight: number, chrome = 10): number {
  return Math.max(6, terminalHeight - Math.floor(terminalHeight / 4) - chrome)
}

export function openTaskDetailsView(api: TuiPluginApi, details: TaskDetails, title = "Task details"): void {
  api.ui.dialog.replace(() => <TaskDetailsDialog api={api} details={details} title={title} />)
}

export function openIntakeNotesView(api: TuiPluginApi, intake: Intake, title = "Intake notes"): void {
  api.ui.dialog.replace(() => <IntakeNotesDialog api={api} intake={intake} title={title} />)
}

function ScrollHint(props: { api: TuiPluginApi }) {
  return <text fg={props.api.theme.current.textMuted}>↑↓ scroll</text>
}

function IntakeNotesDialog(props: { api: TuiPluginApi; intake: Intake; title: string }) {
  const dimensions = useRendererDimensions(props.api)
  onMount(() => props.api.ui.dialog.setSize("large"))
  return (
    <DialogFrame api={props.api} title={props.title} closeLabel="esc close">
      <scrollbox
        maxHeight={dialogScrollMaxHeight(dimensions().height)}
        scrollY={true}
        verticalScrollbarOptions={{ visible: false }}
      >
        <IntakeView intake={props.intake} />
      </scrollbox>
      <ScrollHint api={props.api} />
    </DialogFrame>
  )
}

function TaskDetailsDialog(props: { api: TuiPluginApi; details: TaskDetails; title: string }) {
  const muted = props.api.theme.current.textMuted
  const dimensions = useRendererDimensions(props.api)
  onMount(() => props.api.ui.dialog.setSize("large"))
  return (
    <DialogFrame api={props.api} title={props.title} closeLabel="esc close">
      <box flexDirection="column" flexShrink={0} gap={0}>
        <text wrapMode="word">
          {props.details.taskNumber !== undefined ? `#${props.details.taskNumber} ` : ""}
          {props.details.title || props.details.report || "Untitled task"}
        </text>
        <text fg={muted} wrapMode="word">
          {props.details.status ? `${props.details.status} · ` : ""}
          Generation {props.details.generation} · {props.details.approved ? "approved" : "not approved"}
          {props.details.baseBranch ? ` · base: ${props.details.baseBranch}` : ""}
        </text>
      </box>
      <scrollbox
        maxHeight={dialogScrollMaxHeight(dimensions().height)}
        scrollY={true}
        verticalScrollbarOptions={{ visible: false }}
      >
        <box flexDirection="column" gap={1}>
          {props.details.description ? (
            <text fg={muted} wrapMode="word">
              {props.details.description}
            </text>
          ) : null}
          {props.details.report ? (
            <box flexDirection="column">
              <text attributes={TextAttributes.BOLD}>Report</text>
              <text paddingLeft={2} wrapMode="word">
                {props.details.report}
              </text>
            </box>
          ) : null}
          {props.details.diffStats.length > 0 ? (
            <box flexDirection="column" gap={1}>
              <text attributes={TextAttributes.BOLD}>Changed files ({props.details.diffStats.length})</text>
              <For each={props.details.diffStats}>
                {(stat) => (
                  <text paddingLeft={2} wrapMode="word">
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
      </scrollbox>
      <ScrollHint api={props.api} />
    </DialogFrame>
  )
}
