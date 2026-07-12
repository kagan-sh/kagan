/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { FormState } from "./create-task/types"
import { ScopePicker } from "./create-task/scope-picker"

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

export function openScopePicker(api: TuiPluginApi, scopes: string[], state: FormState, reopenTask: () => void) {
  if (scopes.length === 0) {
    openCustomScopePrompt(api, state, reopenTask)
    return
  }
  const reopenScope = () => openScopePicker(api, scopes, state, reopenTask)
  const onCustom = () => openCustomScopePrompt(api, state, reopenScope)
  api.ui.dialog.replace(() => (
    <ScopePicker
      api={api}
      scopes={scopes}
      state={state}
      reopenTask={reopenTask}
      reopenScope={reopenScope}
      onCustom={onCustom}
    />
  ))
}
