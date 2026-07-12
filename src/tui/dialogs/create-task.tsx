/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { TextareaRenderable } from "@opentui/core"
import { createSignal, onMount, Show } from "solid-js"
import { createTask } from "../tasks"
import { getOrder, setOrder } from "../session/preferences"
import { bunGitRunner, listLocalBranches } from "../../git/runner"
import type { createBoardStore } from "../board/store"
import { useKeyIntercept } from "../renderer"
import { DialogFrame } from "./chrome"
import type { FormState, ModelChoice } from "./create-task-types"
import { openScopePicker } from "./create-task-scope"
import { openFilterableSelectPicker } from "./create-task-select"

type BoardStore = ReturnType<typeof createBoardStore>

type CreateTaskDependencies = {
  createTask: typeof createTask
  getOrder: typeof getOrder
  setOrder: typeof setOrder
  listBranches: (api: TuiPluginApi) => Promise<string[]>
}

function PickerRow(props: { api: TuiPluginApi; label: string; value: string; focused: boolean }) {
  const theme = () => props.api.theme.current
  return (
    <box flexDirection="row" justifyContent="space-between">
      <box flexDirection="row" gap={1}>
        <text fg={props.focused ? theme().primary : theme().textMuted}>{props.label}</text>
        <text fg={theme().text}>{props.value}</text>
      </box>
      <text fg={theme().textMuted}>›</text>
    </box>
  )
}

function hasScope(scope: FormState["scope"]): boolean {
  return scope.values.length > 0 || !!scope.custom
}

function scopeLabel(scope: FormState["scope"]): string {
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
  dependencies: CreateTaskDependencies
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
    if (props.store.configuredScopes.length > 1 && !hasScope(state.scope)) {
      props.store.notify({ variant: "warning", title: "Kagan", message: "Scope is required" })
      return
    }
    props.api.ui.dialog.clear()
    try {
      const scope = hasScope(state.scope) ? state.scope : undefined
      const input = {
        title: trimmed,
        description,
        model: props.models[state.modelIndex]?.model,
        baseBranch: props.branches[state.branchIndex] ?? "HEAD",
        setupCommands: props.store.setupCommands,
        ...(scope ? { scope } : {}),
      }
      const session = await props.dependencies.createTask(props.api, input)
      props.dependencies.setOrder(props.api, "backlog", [
        ...props.dependencies.getOrder(props.api, "backlog"),
        session.id,
      ])
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
    if (focusIndex() === 3) {
      openFilterableSelectPicker(props.api, {
        title: "Model",
        filterPlaceholder: "Filter models",
        labels: props.models.map((choice) => choice.label),
        selectedIndex: state.modelIndex,
        filter: state.modelFilter,
        onFilter: (value) => {
          state.modelFilter = value
        },
        onSelect: (index) => {
          state.modelIndex = index
        },
        reopen: props.reopen,
      })
      return
    }
    openFilterableSelectPicker(props.api, {
      title: "Base branch",
      filterPlaceholder: "Filter branches",
      labels: props.branches,
      selectedIndex: state.branchIndex,
      filter: state.branchFilter,
      onFilter: (value) => {
        state.branchFilter = value
      },
      onSelect: (index) => {
        state.branchIndex = index
      },
      reopen: props.reopen,
    })
  }

  useKeyIntercept(props.api, (key) => {
    if (key.ctrl && key.name === "return") {
      void submit()
      return true
    }
    if (
      focusIndex() === 1 &&
      ((key.ctrl && key.name === "j") || key.name === "linefeed" || (key.shift && key.name === "return"))
    ) {
      descriptionRef?.newLine()
      return true
    }
    if (key.name === "return") {
      if (focusIndex() >= 2) openPicker()
      else void submit()
      return true
    }
    if (key.name === "right" && focusIndex() >= 2) {
      openPicker()
      return true
    }
    if (key.name === "tab") {
      setFocusIndex((index) => (key.shift ? (index + 4) % 5 : (index + 1) % 5))
      return true
    }
    if (focusIndex() === 1) return false
    if (key.name === "down") {
      setFocusIndex((index) => (index + 1) % 5)
      return true
    }
    if (key.name === "up") {
      setFocusIndex((index) => (index + 4) % 5)
      return true
    }
    return false
  })

  const labelColor = (index: number) => (focusIndex() === index ? theme().primary : theme().textMuted)

  return (
    <DialogFrame api={props.api} title="New task">
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
      <PickerRow api={props.api} label="Scope" value={scopeLabel(state.scope)} focused={focusIndex() === 2} />
      <PickerRow
        api={props.api}
        label="Model"
        value={props.models[state.modelIndex]?.label ?? "Auto (session default)"}
        focused={focusIndex() === 3}
      />
      <PickerRow
        api={props.api}
        label="Base branch"
        value={props.branches[state.branchIndex] ?? "HEAD"}
        focused={focusIndex() === 4}
      />
      <box paddingBottom={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          tab <span style={{ fg: theme().textMuted }}>move</span>
        </text>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>create</span>
        </text>
        <Show when={focusIndex() === 1}>
          <text fg={theme().text}>
            ctrl+j <span style={{ fg: theme().textMuted }}>newline</span>
          </text>
        </Show>
        <text fg={theme().text}>
          esc <span style={{ fg: theme().textMuted }}>cancel</span>
        </text>
      </box>
    </DialogFrame>
  )
}

export async function openCreateTaskDialog(
  api: TuiPluginApi,
  store: BoardStore,
  dependencies: CreateTaskDependencies = {
    createTask,
    getOrder,
    setOrder,
    listBranches: (currentApi) => listLocalBranches(bunGitRunner(), currentApi.state.path.worktree),
  },
): Promise<void> {
  const listed = await dependencies.listBranches(api)
  const fallback = api.state.vcs?.branch ?? api.state.vcs?.default_branch ?? "HEAD"
  const branches = listed.length > 0 ? listed : [fallback]
  const models = collectModels(api)
  const configuredScope = store.configuredScopes[0]
  const state: FormState = {
    title: "",
    description: "",
    scope: { values: store.configuredScopes.length === 1 && configuredScope !== undefined ? [configuredScope] : [] },
    scopeFilter: "",
    modelIndex: 0,
    modelFilter: "",
    branchIndex: Math.max(0, branches.indexOf(api.state.vcs?.branch ?? "")),
    branchFilter: "",
    focusIndex: 0,
  }
  const showForm = () => {
    api.ui.dialog.replace(() => (
      <CreateTaskForm
        api={api}
        store={store}
        branches={branches}
        models={models}
        state={state}
        reopen={showForm}
        dependencies={dependencies}
      />
    ))
  }
  showForm()
}
