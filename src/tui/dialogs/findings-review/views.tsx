/** @jsxImportSource @opentui/solid */
import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { For, Show } from "solid-js"
import type { Finding } from "../../../domain/task/findings"
import { FindingRow } from "./row"
export { FindingDetail } from "./detail"

export function FindingsList(props: { theme: TuiThemeCurrent; findings: Finding[]; index: number }) {
  return (
    <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
      <box flexDirection="column" gap={1}>
        <For each={props.findings}>
          {(finding, i) => <FindingRow theme={props.theme} finding={finding} selected={i() === props.index} />}
        </For>
      </box>
    </scrollbox>
  )
}

export function FindingsFooter(props: {
  theme: TuiThemeCurrent
  mode: "list" | "detail"
  clean: boolean
  reason?: string
}) {
  return (
    <box paddingTop={1} flexDirection="row" gap={2}>
      <Show when={props.mode === "list"}>
        <Show when={!props.clean}>
          <box flexDirection="row">
            <text fg={props.theme.text}>enter</text>
            <text fg={props.theme.textMuted}> open</text>
          </box>
        </Show>
        <box flexDirection="row">
          <text fg={props.theme.text}>s</text>
          <text fg={props.theme.textMuted}> send back</text>
        </box>
        <box flexDirection="row">
          <text fg={props.theme.text}>a</text>
          <text fg={props.reason ? props.theme.textMuted : props.theme.success}>
            {" "}
            {props.reason ? "approve" : "approve & merge"}
          </text>
        </box>
        <Show when={props.reason}>
          <text flexShrink={1} wrapMode="none" truncate={true} fg={props.theme.textMuted}>
            ({props.reason})
          </text>
        </Show>
      </Show>
      <Show when={props.mode === "detail"}>
        <box flexDirection="row">
          <text fg={props.theme.text}>tab</text>
          <text fg={props.theme.textMuted}> move</text>
        </box>
        <box flexDirection="row">
          <text fg={props.theme.text}>enter</text>
          <text fg={props.theme.textMuted}> rule</text>
        </box>
        <box flexDirection="row">
          <text fg={props.theme.text}>esc</text>
          <text fg={props.theme.textMuted}> back</text>
        </box>
      </Show>
    </box>
  )
}
