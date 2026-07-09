/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { TextAttributes } from "@opentui/core"
import { For } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { kagan, type Finding, type Intake } from "./task"
import type { ColumnType } from "./types"
import type { CheckResult } from "./check"

export type TaskDetails = {
  title?: string
  status?: ColumnType
  taskNumber?: number
  report?: string
  description?: string
  baseBranch?: string
  generation: number
  approved: boolean
  findings: Finding[]
  priorTriage: Finding[]
  intake?: Intake
  check?: CheckResult
  setup?: CheckResult
  diffStats: Array<{ file: string; additions: number; deletions: number; status?: string }>
}

export function buildTaskDetails(
  metadata: Record<string, unknown>,
  diffs: Array<SnapshotFileDiff>,
  title?: string,
): TaskDetails {
  const view = kagan(metadata)
  return {
    title,
    status: view.status ?? "backlog",
    taskNumber: view.taskNumber,
    report: view.report,
    description: view.description,
    baseBranch: view.baseBranch,
    generation: view.generation,
    approved: view.approved === true,
    findings: view.findings ?? [],
    priorTriage: view.priorTriage ?? [],
    intake: view.intake,
    check: view.check,
    setup: view.setup,
    diffStats: diffs.map((diff) => ({
      file: diff.file ?? "unknown",
      additions: diff.additions ?? 0,
      deletions: diff.deletions ?? 0,
      status: diff.status,
    })),
  }
}

function CheckEvidence(props: { api: TuiPluginApi; label: string; result?: CheckResult }) {
  if (!props.result) return null
  const { command, exitCode, output } = props.result
  const steps = props.result.steps
  return (
    <box flexDirection="column" gap={1}>
      <text attributes={TextAttributes.BOLD}>{props.label}</text>
      {steps && steps.length > 0 ? (
        <For each={steps}>
          {(step) => (
            <box flexDirection="column" paddingLeft={2}>
              <text>
                {step.name} ({step.cwd}) {step.status === "skipped" ? "skipped" : `exited ${step.exitCode ?? "?"}`}
              </text>
              <text fg={props.api.theme.current.textMuted}>{(step.reason ?? step.output).slice(-4000)}</text>
            </box>
          )}
        </For>
      ) : (
        <>
          <text>
            `{command}` exited {exitCode === null ? "?" : exitCode}
          </text>
          <text fg={props.api.theme.current.textMuted}>{output.slice(-4000)}</text>
        </>
      )}
    </box>
  )
}

function FindingList(props: { api: TuiPluginApi; title: string; findings: Finding[] }) {
  if (props.findings.length === 0) return null
  return (
    <box flexDirection="column" gap={1}>
      <text attributes={TextAttributes.BOLD}>
        {props.title} ({props.findings.length})
      </text>
      <For each={props.findings}>
        {(finding) => (
          <box flexDirection="column" paddingLeft={2}>
            <text>
              {finding.category ? `[${finding.category}] ` : ""}
              {finding.summary}
              {finding.resolution ? ` — ${finding.resolution}` : ""}
            </text>
            {finding.note ? (
              <text fg={props.api.theme.current.textMuted} paddingLeft={2}>
                {finding.note}
              </text>
            ) : null}
          </box>
        )}
      </For>
    </box>
  )
}

function IntakeView(props: { intake?: Intake }) {
  if (!props.intake) return null
  const resolved = props.intake.decisions.filter((d) => d.resolution === "approved" || d.resolution === "overridden")
  return (
    <box flexDirection="column" gap={1}>
      <text attributes={TextAttributes.BOLD}>Intake</text>
      {props.intake.understanding ? <text paddingLeft={2}>{props.intake.understanding}</text> : null}
      {resolved.length > 0 ? (
        <box flexDirection="column" paddingLeft={2}>
          <text>Resolved decisions:</text>
          <For each={resolved}>
            {(d) => (
              <text>
                - {d.question} → {d.resolution === "approved" ? d.assumption : (d.answer ?? "")}
              </text>
            )}
          </For>
        </box>
      ) : null}
    </box>
  )
}

export function openTaskDetailsView(api: TuiPluginApi, details: TaskDetails, title = "Task details"): void {
  const muted = api.theme.current.textMuted
  api.ui.dialog.replace(() => (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} gap={1}>
      <box flexDirection="row" justifyContent="space-between">
        <text attributes={TextAttributes.BOLD}>{title}</text>
        <text fg={muted}>esc close</text>
      </box>
      <text>
        {details.taskNumber !== undefined ? `#${details.taskNumber} ` : ""}
        {details.title || details.report || "Untitled task"}
      </text>
      <text>
        {details.status ? `${details.status} · ` : ""}
        Generation {details.generation} · {details.approved ? "approved" : "not approved"}
        {details.baseBranch ? ` · base: ${details.baseBranch}` : ""}
      </text>
      {details.description ? <text fg={muted}>{details.description}</text> : null}
      {details.diffStats.length > 0 ? (
        <box flexDirection="column" gap={1}>
          <text attributes={TextAttributes.BOLD}>Changed files ({details.diffStats.length})</text>
          <For each={details.diffStats}>
            {(stat) => (
              <text paddingLeft={2}>
                {stat.file} (+{stat.additions}/-{stat.deletions}){stat.status ? ` · ${stat.status}` : ""}
              </text>
            )}
          </For>
        </box>
      ) : null}
      <CheckEvidence api={api} label="Setup" result={details.setup} />
      <CheckEvidence api={api} label="Check" result={details.check} />
      <IntakeView intake={details.intake} />
      <FindingList api={api} title="Findings" findings={details.findings} />
      <FindingList api={api} title="Prior triage" findings={details.priorTriage} />
    </box>
  ))
}
