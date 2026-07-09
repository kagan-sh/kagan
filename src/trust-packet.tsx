/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { TextAttributes } from "@opentui/core"
import { For } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { kagan, type Finding, type Intake } from "./task"
import type { ColumnType } from "./types"
import type { CheckResult } from "./check"

export type TrustPacket = {
  version: 1
  exportedAt: string
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

export function serializeTrustPacket(
  metadata: Record<string, unknown>,
  diffs: Array<SnapshotFileDiff>,
  title?: string,
): TrustPacket {
  const view = kagan(metadata)
  return {
    version: 1,
    exportedAt: new Date().toISOString(),
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

export function isTrustPacket(value: unknown): value is TrustPacket {
  if (typeof value !== "object" || value === null) return false
  const packet = value as Partial<TrustPacket>
  return (
    packet.version === 1 &&
    typeof packet.exportedAt === "string" &&
    typeof packet.generation === "number" &&
    typeof packet.approved === "boolean" &&
    Array.isArray(packet.findings) &&
    Array.isArray(packet.priorTriage) &&
    Array.isArray(packet.diffStats)
  )
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

export function openTrustPacketView(api: TuiPluginApi, packet: TrustPacket, title = "Trust packet — view only"): void {
  const muted = api.theme.current.textMuted
  api.ui.dialog.replace(() => (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} gap={1}>
      <box flexDirection="row" justifyContent="space-between">
        <text attributes={TextAttributes.BOLD}>{title}</text>
        <text fg={muted}>esc close</text>
      </box>
      <text fg={muted}>Exported {packet.exportedAt}</text>
      <text>
        {packet.taskNumber !== undefined ? `#${packet.taskNumber} ` : ""}
        {packet.title || packet.report || "Untitled task"}
      </text>
      <text>
        {packet.status ? `${packet.status} · ` : ""}
        Generation {packet.generation} · {packet.approved ? "approved" : "not approved"}
        {packet.baseBranch ? ` · base: ${packet.baseBranch}` : ""}
      </text>
      {packet.description ? <text fg={muted}>{packet.description}</text> : null}
      {packet.diffStats.length > 0 ? (
        <box flexDirection="column" gap={1}>
          <text attributes={TextAttributes.BOLD}>Changed files ({packet.diffStats.length})</text>
          <For each={packet.diffStats}>
            {(stat) => (
              <text paddingLeft={2}>
                {stat.file} (+{stat.additions}/-{stat.deletions}){stat.status ? ` · ${stat.status}` : ""}
              </text>
            )}
          </For>
        </box>
      ) : null}
      <CheckEvidence api={api} label="Setup" result={packet.setup} />
      <CheckEvidence api={api} label="Check" result={packet.check} />
      <IntakeView intake={packet.intake} />
      <FindingList api={api} title="Findings" findings={packet.findings} />
      <FindingList api={api} title="Prior triage" findings={packet.priorTriage} />
    </box>
  ))
}
