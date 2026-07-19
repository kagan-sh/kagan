/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi, TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { For, Show, createSignal, onMount } from "solid-js"
import { useKeyIntercept, useRendererDimensions } from "../renderer"
import { DialogFrame } from "./chrome"
import { MarkdownBody } from "./intake-gate-content"

const WIDE_MIN_WIDTH = 72
const ACTION_MIN_WIDTH = 24
const COLUMN_GAP = 2
/** Matches OpenCode host Dialog panel widths (`packages/tui/src/ui/dialog.tsx`). */
const HOST_DIALOG_WIDTH = { medium: 60, large: 88, xlarge: 116 } as const
/** DialogFrame paddingLeft + paddingRight */
const FRAME_PAD = 4

export function dialogContentWidth(terminalWidth: number, size: keyof typeof HOST_DIALOG_WIDTH = "large"): number {
  const panel = Math.min(HOST_DIALOG_WIDTH[size], Math.max(2, terminalWidth - 2))
  return Math.max(20, panel - FRAME_PAD)
}

function ActionList(props: { theme: TuiThemeCurrent; labels: string[]; index: number; footer?: string }) {
  return (
    <box flexDirection="column" gap={1} minWidth={ACTION_MIN_WIDTH} flexShrink={0}>
      <For each={props.labels}>
        {(label, i) => (
          <box backgroundColor={i() === props.index ? props.theme.primary : undefined} paddingLeft={1} paddingRight={1}>
            <text fg={i() === props.index ? props.theme.selectedListItemText : props.theme.text}>
              {i() === props.index ? `▸ ${label}` : `  ${label}`}
            </text>
          </box>
        )}
      </For>
      <Show when={props.footer}>
        <text fg={props.theme.textMuted}>{props.footer}</text>
      </Show>
    </box>
  )
}

function handleChoiceKeys(
  key: { name: string },
  labels: string[],
  index: () => number,
  setIndex: (value: number | ((prev: number) => number)) => number,
  onChoose: (index: number) => void,
  onCancel: () => void,
): boolean {
  if (key.name === "escape") {
    onCancel()
    return true
  }
  if (key.name === "down" || key.name === "j") {
    setIndex((value) => Math.min(value + 1, labels.length - 1))
    return true
  }
  if (key.name === "up" || key.name === "k") {
    setIndex((value) => Math.max(value - 1, 0))
    return true
  }
  if (key.name === "return") {
    onChoose(index())
    return true
  }
  if (key.name === "1" || key.name === "2") {
    const choice = Number(key.name) - 1
    if (choice >= 0 && choice < labels.length) {
      onChoose(choice)
      return true
    }
  }
  return false
}

export function TwoColumnGate(props: {
  api: TuiPluginApi
  title: string
  markdown: string
  labels: string[]
  onChoose: (index: number) => void
  onCancel: () => void
}) {
  const theme = () => props.api.theme.current
  const dimensions = useRendererDimensions(props.api)
  const bodyWidth = () => dialogContentWidth(dimensions().width, "large")
  const wide = () => bodyWidth() >= WIDE_MIN_WIDTH
  const [index, setIndex] = createSignal(0)
  const leftWidth = () => Math.max(24, bodyWidth() - ACTION_MIN_WIDTH - COLUMN_GAP)

  onMount(() => props.api.ui.dialog.setSize("large"))
  useKeyIntercept(props.api, (key) =>
    handleChoiceKeys(key, props.labels, index, setIndex, props.onChoose, props.onCancel),
  )

  return (
    <DialogFrame api={props.api} title={props.title}>
      <Show
        when={wide()}
        fallback={
          <box flexDirection="column" gap={1} width={bodyWidth()}>
            <MarkdownBody api={props.api} content={props.markdown} width={bodyWidth()} />
            <ActionList theme={theme()} labels={props.labels} index={index()} footer="j/k  enter  esc" />
          </box>
        }
      >
        <box flexDirection="row" gap={COLUMN_GAP} width={bodyWidth()} alignItems="flex-start">
          <box width={leftWidth()} flexShrink={0}>
            <MarkdownBody api={props.api} content={props.markdown} width={leftWidth()} />
          </box>
          <box width={ACTION_MIN_WIDTH} flexShrink={0}>
            <ActionList theme={theme()} labels={props.labels} index={index()} footer="j/k  enter  esc" />
          </box>
        </box>
      </Show>
    </DialogFrame>
  )
}

export function IntakeAnswerGate(props: {
  api: TuiPluginApi
  title: string
  markdown: string
  onSubmit: (answer: string) => void
  onBack: () => void
}) {
  const theme = () => props.api.theme.current
  const [answer, setAnswer] = createSignal("")
  const dimensions = useRendererDimensions(props.api)
  const bodyWidth = () => dialogContentWidth(dimensions().width, "large")

  onMount(() => props.api.ui.dialog.setSize("large"))
  useKeyIntercept(props.api, (key) => {
    if (key.name === "escape") {
      props.onBack()
      return true
    }
    return false
  })

  return (
    <DialogFrame api={props.api} title={props.title}>
      <box flexDirection="column" gap={1} width={bodyWidth()}>
        <MarkdownBody api={props.api} content={props.markdown} width={bodyWidth()} />
        <box flexDirection="column" gap={1} width={bodyWidth()}>
          <text fg={theme().text} attributes={TextAttributes.BOLD}>
            Answer
          </text>
          <input
            focused={true}
            value={answer()}
            onInput={setAnswer}
            placeholder="Override the assumption (required)"
            onSubmit={(value) => props.onSubmit(typeof value === "string" ? value : answer())}
          />
          <text fg={theme().textMuted}>enter submit · esc back</text>
        </box>
      </box>
    </DialogFrame>
  )
}
