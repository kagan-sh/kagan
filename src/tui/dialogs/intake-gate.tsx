/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { IntakeDecision } from "../../domain/task/intake"
import type { BoardSession } from "../types"
import { answerMarkdown, decisionMarkdown, modeMarkdown, taskRef } from "./intake-gate-content"
import { IntakeAnswerGate, TwoColumnGate } from "./intake-gate-views"

export {
  answerMarkdown,
  configureIntakeMarkdownTreeSitter,
  decisionMarkdown,
  modeMarkdown,
  taskRef,
} from "./intake-gate-content"

export function openIntakeDecisionDialog(
  api: TuiPluginApi,
  props: {
    session: BoardSession
    index: number
    total: number
    decision: IntakeDecision
    onApprove: () => void
    onReject: () => void
    onCancel: () => void
  },
): void {
  const title = `Intake decision (${props.index + 1}/${props.total}) · ${taskRef(props.session)}`
  api.ui.dialog.replace(() => (
    <TwoColumnGate
      api={api}
      title={title}
      markdown={decisionMarkdown(props.decision)}
      labels={["Approve", "Reject & answer"]}
      onCancel={props.onCancel}
      onChoose={(choice) => {
        if (choice === 0) props.onApprove()
        else props.onReject()
      }}
    />
  ))
}

export function openIntakeAnswerDialog(
  api: TuiPluginApi,
  props: {
    session: BoardSession
    index: number
    total: number
    decision: IntakeDecision
    onSubmit: (answer: string) => void
    onBack: () => void
  },
): void {
  const title = `Your answer · overrides assumption (${props.index + 1}/${props.total}) · ${taskRef(props.session)}`
  api.ui.dialog.replace(() => (
    <IntakeAnswerGate
      api={api}
      title={title}
      markdown={answerMarkdown(props.decision)}
      onSubmit={props.onSubmit}
      onBack={props.onBack}
    />
  ))
}

export function openIntakeModeConfirmDialog(
  api: TuiPluginApi,
  props: {
    session: BoardSession
    rationale: string
    recommended: "assisted" | "manual"
    onConfirm: () => void
    onCancel: () => void
  },
): void {
  const label = props.recommended === "manual" ? "Manual mode" : "Assisted mode"
  const title = `${label} · ${taskRef(props.session)}`
  api.ui.dialog.replace(() => (
    <TwoColumnGate
      api={api}
      title={title}
      markdown={modeMarkdown(props.rationale, props.recommended)}
      labels={["Start agent anyway", "Keep in backlog"]}
      onCancel={props.onCancel}
      onChoose={(choice) => {
        if (choice === 0) props.onConfirm()
        else props.onCancel()
      }}
    />
  ))
}
