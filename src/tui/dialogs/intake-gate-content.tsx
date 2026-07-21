/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { getTreeSitterClient, type TreeSitterClient } from "@opentui/core"
import { createMemo } from "solid-js"
import type { IntakeDecision } from "../../domain/task/intake"
import { kagan } from "../../domain/task/metadata"
import type { BoardSession } from "../types"
import { syntaxStyleFromTheme } from "./markdown-style"

let markdownTreeSitter: TreeSitterClient | undefined

/** Test seam: OpenTUI's shared Tree-sitter client is destroyed with the test renderer. */
export function configureIntakeMarkdownTreeSitter(client?: TreeSitterClient) {
  markdownTreeSitter = client
}

function intakeTreeSitterClient(): TreeSitterClient | undefined {
  if (markdownTreeSitter) return markdownTreeSitter
  try {
    return getTreeSitterClient()
  } catch {
    return undefined
  }
}

export function taskRef(session: BoardSession): string {
  const number = kagan(session.metadata).taskNumber
  if (number !== undefined) return `#${number}`
  return session.title || session.slug || session.id
}

export function decisionMarkdown(decision: IntakeDecision): string {
  return `## Assumption\n\n${decision.assumption}\n\n## Question\n\n${decision.question}`
}

export function answerMarkdown(decision: IntakeDecision): string {
  return `## Question\n\n${decision.question}\n\n### Overriding assumption\n\n${decision.assumption}`
}

export function modeMarkdown(rationale: string, recommended: "assisted" | "manual"): string {
  const heading = recommended === "manual" ? "Why manual" : "Why assisted"
  return `## ${heading}\n\n${rationale}`
}

export function MarkdownBody(props: { api: TuiPluginApi; content: string; width: number }) {
  const style = createMemo(() => syntaxStyleFromTheme(props.api.theme.current))
  const client = () => intakeTreeSitterClient()
  return (
    <markdown content={props.content} syntaxStyle={style()} conceal width={props.width} treeSitterClient={client()} />
  )
}
