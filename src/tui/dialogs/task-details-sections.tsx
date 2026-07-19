/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { TextAttributes } from "@opentui/core"
import { For } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { getStatus, kagan } from "../../domain/task/metadata"
import type { Finding } from "../../domain/task/findings"
import type { Intake } from "../../domain/task/intake"
import type { ColumnType } from "../../domain/task/types"
import type { CheckResult } from "../../checks/runner"

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
    status: getStatus(metadata),
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

export function CheckEvidence(props: { api: TuiPluginApi; label: string; result?: CheckResult }) {
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

export function FindingList(props: { api: TuiPluginApi; title: string; findings: Finding[] }) {
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

export function IntakeView(props: { intake?: Intake }) {
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
