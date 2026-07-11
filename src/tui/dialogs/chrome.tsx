/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import type { ParentProps } from "solid-js"

export function DialogFrame(props: ParentProps<{ api: TuiPluginApi; title: string; closeLabel?: string }>) {
  const theme = () => props.api.theme.current
  return (
    <box paddingLeft={2} paddingRight={2} gap={1}>
      <box flexDirection="row" justifyContent="space-between">
        <text fg={theme().text} attributes={TextAttributes.BOLD}>
          {props.title}
        </text>
        <text fg={theme().textMuted}>{props.closeLabel ?? "esc"}</text>
      </box>
      {props.children}
    </box>
  )
}

export function DialogFilter(props: {
  api: TuiPluginApi
  value: string
  placeholder: string
  onInput: (value: string) => void
}) {
  const theme = () => props.api.theme.current
  return (
    <box flexDirection="column">
      <text fg={theme().textMuted}>filter</text>
      <input focused={true} value={props.value} onInput={props.onInput} placeholder={props.placeholder} />
    </box>
  )
}
