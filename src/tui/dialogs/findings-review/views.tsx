/** @jsxImportSource @opentui/solid */
import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { For, Show } from "solid-js"
import type { Finding } from "../../../domain/task/findings"
import { FindingRow } from "./row"
export { FindingDetail } from "./detail"

export type FindingsMode = "list" | "detail"

const LIST_MAX_HEIGHT = 12

export function FindingsList(props: { theme: TuiThemeCurrent; findings: Finding[]; index: number }) {
  return (
    <scrollbox maxHeight={LIST_MAX_HEIGHT} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
      <box flexDirection="column" gap={1}>
        <For each={props.findings}>
          {(finding, i) => <FindingRow theme={props.theme} finding={finding} selected={i() === props.index} />}
        </For>
      </box>
    </scrollbox>
  )
}

export function FindingsFooter(props: { theme: TuiThemeCurrent; mode: FindingsMode; clean: boolean; reason?: string }) {
  const ready = () => props.mode === "list" && !props.reason
  return (
    <box flexDirection="column" gap={1} flexShrink={0}>
      <Show when={props.mode === "list" && props.reason}>
        <text fg={props.theme.warning}>{props.reason} — enter opens a finding</text>
      </Show>
      <Show when={ready()}>
        <box backgroundColor={props.theme.primary} paddingLeft={1} paddingRight={1} flexShrink={0}>
          <text fg={props.theme.selectedListItemText}>▸ Approve & merge — a</text>
        </box>
      </Show>
      <box flexDirection="row" gap={2} flexShrink={0}>
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
          <Show when={!ready()}>
            <box flexDirection="row">
              <text fg={props.theme.textMuted}>a</text>
              <text fg={props.theme.textMuted}> approve</text>
            </box>
          </Show>
          <box flexDirection="row">
            <text fg={props.theme.text}>esc</text>
            <text fg={props.theme.textMuted}> close</text>
          </box>
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
    </box>
  )
}
