/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createMemo, createSignal, For, onMount, Show } from "solid-js"
import { useKeyIntercept } from "../../renderer"
import { DialogFilter, DialogFrame } from "../chrome"
import type { FormState } from "./types"

export function ScopePicker(props: {
  api: TuiPluginApi
  scopes: string[]
  state: FormState
  reopenTask: () => void
  reopenScope: () => void
  onCustom: () => void
}) {
  const theme = () => props.api.theme.current
  const [filter, setFilter] = createSignal(props.state.scopeFilter)
  const [index, setIndex] = createSignal(0)
  const options = createMemo(() => {
    const query = filter().trim().toLowerCase()
    const filtered = query ? props.scopes.filter((scope) => scope.toLowerCase().includes(query)) : props.scopes
    return [...filtered, "custom..."]
  })
  const close = () => {
    props.state.scopeFilter = filter()
    props.reopenTask()
  }
  const toggle = (scope: string) => {
    const values = props.state.scope.values.includes(scope)
      ? props.state.scope.values.filter((value) => value !== scope)
      : [...props.state.scope.values, scope]
    props.state.scope = { ...props.state.scope, values }
  }

  onMount(() => props.api.ui.dialog.setSize("medium"))

  useKeyIntercept(props.api, (key) => {
    if (key.name === "escape") {
      close()
      return true
    }
    if (key.name === "down") {
      setIndex((value) => Math.min(value + 1, options().length - 1))
      return true
    }
    if (key.name === "up") {
      setIndex((value) => Math.max(value - 1, 0))
      return true
    }
    if (key.name === " " || key.name === "space") {
      const scope = options()[index()]
      if (!scope) return false
      if (scope === "custom...") props.onCustom()
      else toggle(scope)
      return true
    }
    if (key.name === "return") {
      close()
      return true
    }
    return false
  })

  return (
    <DialogFrame api={props.api} title="Scope">
      <DialogFilter api={props.api} value={filter()} onInput={setFilter} placeholder="Filter configured scopes" />
      <box flexDirection="column">
        <For each={options()}>
          {(scope, i) => {
            const selected = () => i() === index()
            const checked = () => scope !== "custom..." && props.state.scope.values.includes(scope)
            return (
              <box backgroundColor={selected() ? theme().primary : undefined}>
                <text fg={selected() ? theme().selectedListItemText : theme().text}>
                  {scope === "custom..." ? "  " : checked() ? "✓ " : "  "}
                  {scope}
                </text>
              </box>
            )
          }}
        </For>
        <Show when={props.state.scope.custom}>
          <text fg={theme().textMuted}>custom: {props.state.scope.custom}</text>
        </Show>
      </box>
      <box flexDirection="row" gap={2}>
        <text fg={theme().text}>
          space <span style={{ fg: theme().textMuted }}>toggle/custom</span>
        </text>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>apply</span>
        </text>
      </box>
    </DialogFrame>
  )
}
