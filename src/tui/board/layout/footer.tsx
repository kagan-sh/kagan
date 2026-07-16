/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { For, Show } from "solid-js"
import { version } from "../../../../package.json"
import type { BoardStore } from "../commands/context"
import { updateFooter } from "./footer-text"

export function Footer(props: { api: TuiPluginApi; store: BoardStore; hints: () => { key: string; label: string }[] }) {
  const theme = () => props.api.theme.current
  const filter = () => props.store.filter()

  return (
    <box flexDirection="row" flexShrink={0} paddingLeft={2} paddingRight={2} justifyContent="space-between">
      <text wrapMode="none" truncate={true} fg={theme().textMuted}>
        kagan v{version}
        <Show when={props.store.updateStatus()}>
          {(status) => (
            <span style={{ fg: status().kind === "restart" ? theme().success : theme().info }}>
              {updateFooter(status())}
            </span>
          )}
        </Show>
        <Show when={filter()}>{` · filter: ${filter()}`}</Show>
      </text>
      <box flexDirection="row" gap={2} flexShrink={0}>
        <For each={props.hints()}>
          {(hint) => (
            <text wrapMode="none" truncate={true}>
              <span style={{ fg: theme().text }}>{hint.key}</span>
              <span style={{ fg: theme().textMuted }}> {hint.label}</span>
            </text>
          )}
        </For>
      </box>
    </box>
  )
}
