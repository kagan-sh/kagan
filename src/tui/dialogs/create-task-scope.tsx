/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createMemo, createSignal, For, onMount, Show } from "solid-js"
import { useKeyIntercept } from "../renderer"
import { DialogFilter, DialogFrame } from "./chrome"
import type { FormState } from "./create-task-types"

function openCustomScopePrompt(api: TuiPluginApi, state: FormState, reopenScope: () => void) {
  api.ui.dialog.replace(() => (
    <api.ui.DialogPrompt
      title="Custom scope"
      placeholder="docs, infra, shared config..."
      value={state.scope.custom ?? ""}
      onConfirm={(value) => {
        const custom = value.trim()
        state.scope = { ...state.scope, ...(custom ? { custom } : { custom: undefined }) }
        reopenScope()
      }}
      onCancel={reopenScope}
    />
  ))
}

function ScopePicker(props: {
  api: TuiPluginApi
  scopes: string[]
  state: FormState
  reopenTask: () => void
  reopenScope: () => void
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
      if (scope === "custom...") openCustomScopePrompt(props.api, props.state, props.reopenScope)
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
      <box paddingBottom={1} flexDirection="row" gap={2}>
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

export function openScopePicker(api: TuiPluginApi, scopes: string[], state: FormState, reopenTask: () => void) {
  if (scopes.length === 0) {
    openCustomScopePrompt(api, state, reopenTask)
    return
  }
  const reopenScope = () => openScopePicker(api, scopes, state, reopenTask)
  api.ui.dialog.replace(() => (
    <ScopePicker api={api} scopes={scopes} state={state} reopenTask={reopenTask} reopenScope={reopenScope} />
  ))
}
