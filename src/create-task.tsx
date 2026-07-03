/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { useKeyboard } from "@opentui/solid"
import { TextAttributes, type TextareaRenderable } from "@opentui/core"
import { createSignal, onMount } from "solid-js"
import { createTask, getOrder, setOrder } from "./session-api"
import { bunGitRunner, listLocalBranches } from "./git"
import type { ModelRef } from "./task"
import type { createBoardStore } from "./store"

type BoardStore = ReturnType<typeof createBoardStore>

type ModelChoice = { label: string; model?: ModelRef }

type FormState = {
  title: string
  description: string
  modelIndex: number
  branchIndex: number
  focusIndex: number
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
      const session = await createTask(props.api, {
        title: trimmed,
        description,
        model: props.models[state.modelIndex]?.model,
        baseBranch: props.branches[state.branchIndex] ?? "HEAD",
        setupCommand: props.store.setupCommand,
      })
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
    const model = focusIndex() === 2
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
      setFocusIndex((index) => (key.shift ? (index + 3) % 4 : (index + 1) % 4))
      return
    }
    if (focusIndex() === 1) return
    if (key.name === "down") {
      setFocusIndex((index) => (index + 1) % 4)
      return
    }
    if (key.name === "up") setFocusIndex((index) => (index + 3) % 4)
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
          <text fg={labelColor(2)}>Model</text>
          <text fg={theme().text}>{props.models[state.modelIndex]?.label ?? "Auto (session default)"}</text>
        </box>
        <text fg={theme().textMuted}>›</text>
      </box>
      <box flexDirection="row" justifyContent="space-between">
        <box flexDirection="row" gap={1}>
          <text fg={labelColor(3)}>Base branch</text>
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
