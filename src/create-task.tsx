/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { useKeyboard } from "@opentui/solid"
import { TextAttributes, type TextareaRenderable } from "@opentui/core"
import { createMemo, createSignal, For, onMount, Show } from "solid-js"
import { createTask, getOrder, setOrder } from "./session-api"
import { bunGitRunner, listLocalBranches } from "./git"
import type { ModelRef, TaskScope } from "./task"
import type { createBoardStore } from "./store"

type BoardStore = ReturnType<typeof createBoardStore>

type ModelChoice = { label: string; model?: ModelRef }

type FormState = {
  title: string
  description: string
  scope: TaskScope
  scopeFilter: string
  modelIndex: number
  branchIndex: number
  focusIndex: number
}

function scopeLabel(scope: TaskScope): string {
  const parts = [...scope.values]
  if (scope.custom) parts.push(scope.custom)
  return parts.length > 0 ? parts.join(", ") : "Not set"
}

function collectModels(api: TuiPluginApi): ModelChoice[] {
  const choices: ModelChoice[] = [{ label: "Auto (session default)" }]
  for (const provider of api.state.provider) {
    for (const model of Object.values(provider.models)) {
      choices.push({ label: `${provider.id}/${model.id}`, model: { providerID: provider.id, modelID: model.id } })
    }
  }
  return choices
}

function CreateTaskForm(props: {
  api: TuiPluginApi
  store: BoardStore
  branches: string[]
  models: ModelChoice[]
  state: FormState
  reopen: () => void
}) {
  const theme = () => props.api.theme.current
  const state = props.state
  const [focusIndex, setFocusIndex] = createSignal(state.focusIndex)
  let descriptionRef: TextareaRenderable | undefined

  onMount(() => props.api.ui.dialog.setSize("medium"))

  const submit = async () => {
    const trimmed = state.title.trim()
    if (!trimmed) {
      props.store.notify({ variant: "warning", title: "Kagan", message: "Title is required" })
      return
    }
    const description = descriptionRef?.plainText ?? state.description
    props.api.ui.dialog.clear()
    try {
      const scope = scopeLabel(state.scope) === "Not set" ? undefined : state.scope
      const input = {
        title: trimmed,
        description,
        model: props.models[state.modelIndex]?.model,
        baseBranch: props.branches[state.branchIndex] ?? "HEAD",
        setupCommands: props.store.setupCommands,
        ...(scope ? { scope } : {}),
      }
      const session = await createTask(props.api, input)
      setOrder(props.api, "backlog", [...getOrder(props.api, "backlog"), session.id])
      await props.store.refresh()
      props.store.notify({ variant: "success", title: "Kagan", message: `Created "${trimmed}"` })
    } catch (error) {
      props.store.notify({
        variant: "error",
        title: "Kagan",
        message: error instanceof Error ? error.message : String(error),
      })
    }
  }

  const openPicker = () => {
    state.focusIndex = focusIndex()
    state.description = descriptionRef?.plainText ?? state.description
    if (focusIndex() === 2) {
      openScopePicker(props.api, props.store.configuredScopes, state, props.reopen)
      return
    }
    const model = focusIndex() === 3
    const options = model
      ? props.models.map((choice, index) => ({ title: choice.label, value: index }))
      : props.branches.map((branch, index) => ({ title: branch, value: index }))
    const current = model ? state.modelIndex : state.branchIndex
    let settled = false
    props.api.ui.dialog.replace(
      () => (
        <props.api.ui.DialogSelect<number>
          title={model ? "Model" : "Base branch"}
          options={options}
          current={current}
          onSelect={(option) => {
            if (model) state.modelIndex = option.value
            else state.branchIndex = option.value
            settled = true
            props.reopen()
          }}
        />
      ),
      () => {
        // dialog.replace() calls onClose on every dialog it displaces, and the escape
        // keybinding pops the stack right after calling onClose - reopening the form
        // synchronously here would replace it back in, then have it popped straight back
        // out. Deferring past that pop (and guarding re-entry from the replace() below)
        // leaves exactly the form on the stack, whichever path triggered the close.
        if (settled) return
        settled = true
        queueMicrotask(() => props.reopen())
      },
    )
  }

  useKeyboard((key) => {
    if (key.ctrl && key.name === "return") {
      void submit()
      return
    }
    if (key.name === "return") {
      if (focusIndex() === 1) return
      if (focusIndex() >= 2) openPicker()
      else void submit()
      return
    }
    if (key.name === "right" && focusIndex() >= 2) {
      openPicker()
      return
    }
    if (key.name === "tab") {
      setFocusIndex((index) => (key.shift ? (index + 4) % 5 : (index + 1) % 5))
      return
    }
    if (focusIndex() === 1) return
    if (key.name === "down") {
      setFocusIndex((index) => (index + 1) % 5)
      return
    }
    if (key.name === "up") setFocusIndex((index) => (index + 4) % 5)
  })

  const labelColor = (index: number) => (focusIndex() === index ? theme().primary : theme().textMuted)

  return (
    <box paddingLeft={2} paddingRight={2} gap={1}>
      <box flexDirection="row" justifyContent="space-between">
        <text fg={theme().text} attributes={TextAttributes.BOLD}>
          New task
        </text>
        <text fg={theme().textMuted}>esc</text>
      </box>
      <box flexDirection="column">
        <text fg={labelColor(0)}>Title</text>
        <input
          focused={focusIndex() === 0}
          value={state.title}
          placeholder="What should the agent do?"
          onInput={(value) => {
            state.title = value
          }}
        />
      </box>
      <box flexDirection="column">
        <text fg={labelColor(1)}>Description</text>
        <textarea
          height={3}
          focused={focusIndex() === 1}
          initialValue={state.description}
          placeholder="Optional context or constraints"
          ref={(el: TextareaRenderable) => {
            descriptionRef = el
          }}
        />
      </box>
      <box flexDirection="row" justifyContent="space-between">
        <box flexDirection="row" gap={1}>
          <text fg={labelColor(2)}>Scope</text>
          <text fg={theme().text}>{scopeLabel(state.scope)}</text>
        </box>
        <text fg={theme().textMuted}>›</text>
      </box>
      <box flexDirection="row" justifyContent="space-between">
        <box flexDirection="row" gap={1}>
          <text fg={labelColor(3)}>Model</text>
          <text fg={theme().text}>{props.models[state.modelIndex]?.label ?? "Auto (session default)"}</text>
        </box>
        <text fg={theme().textMuted}>›</text>
      </box>
      <box flexDirection="row" justifyContent="space-between">
        <box flexDirection="row" gap={1}>
          <text fg={labelColor(4)}>Base branch</text>
          <text fg={theme().text}>{props.branches[state.branchIndex] ?? "HEAD"}</text>
        </box>
        <text fg={theme().textMuted}>›</text>
      </box>
      <box paddingBottom={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          tab <span style={{ fg: theme().textMuted }}>move</span>
        </text>
        <text fg={theme().text}>
          {focusIndex() === 0 ? "enter" : "ctrl+enter"} <span style={{ fg: theme().textMuted }}>create</span>
        </text>
        <text fg={theme().text}>
          esc <span style={{ fg: theme().textMuted }}>cancel</span>
        </text>
      </box>
    </box>
  )
}

export async function openCreateTaskDialog(api: TuiPluginApi, store: BoardStore): Promise<void> {
  const listed = await listLocalBranches(bunGitRunner(), api.state.path.worktree)
  const fallback = api.state.vcs?.branch ?? api.state.vcs?.default_branch ?? "HEAD"
  const branches = listed.length > 0 ? listed : [fallback]
  const models = collectModels(api)
  const state: FormState = {
    title: "",
    description: "",
    scope: { values: [] },
    scopeFilter: "",
    modelIndex: 0,
    branchIndex: Math.max(0, branches.indexOf(api.state.vcs?.branch ?? "")),
    focusIndex: 0,
  }
  const showForm = () => {
    api.ui.dialog.replace(() => (
      <CreateTaskForm api={api} store={store} branches={branches} models={models} state={state} reopen={showForm} />
    ))
  }
  showForm()
}

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

  useKeyboard((key) => {
    if (key.name === "escape") {
      close()
      return
    }
    if (key.name === "down") {
      setIndex((value) => Math.min(value + 1, options().length - 1))
      return
    }
    if (key.name === "up") {
      setIndex((value) => Math.max(value - 1, 0))
      return
    }
    if (key.name === " " || key.name === "space") {
      const scope = options()[index()]
      if (!scope) return
      if (scope === "custom...") openCustomScopePrompt(props.api, props.state, props.reopenScope)
      else toggle(scope)
      return
    }
    if (key.name === "return") close()
  })

  return (
    <box paddingLeft={2} paddingRight={2} gap={1}>
      <box flexDirection="row" justifyContent="space-between">
        <text fg={theme().text} attributes={TextAttributes.BOLD}>
          Scope
        </text>
        <text fg={theme().textMuted}>esc</text>
      </box>
      <box flexDirection="column">
        <text fg={theme().textMuted}>filter</text>
        <input focused={true} value={filter()} onInput={setFilter} placeholder="Filter configured scopes" />
      </box>
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
    </box>
  )
}

function openScopePicker(api: TuiPluginApi, scopes: string[], state: FormState, reopenTask: () => void) {
  if (scopes.length === 0) {
    openCustomScopePrompt(api, state, reopenTask)
    return
  }
  const reopenScope = () => openScopePicker(api, scopes, state, reopenTask)
  api.ui.dialog.replace(() => (
    <ScopePicker api={api} scopes={scopes} state={state} reopenTask={reopenTask} reopenScope={reopenScope} />
  ))
}
