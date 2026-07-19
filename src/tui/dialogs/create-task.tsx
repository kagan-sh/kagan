/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createTask } from "../tasks"
import { getOrder, setOrder } from "../session/preferences"
import { bunGitRunner, listLocalBranches } from "../../git/runner"
import type { BoardStore } from "../board/store"
import type { CreateTaskDependencies, FormState, ModelChoice } from "./create-task/form-actions"
import { CreateTaskForm } from "./create-task/form"

function collectModels(api: TuiPluginApi): ModelChoice[] {
  const choices: ModelChoice[] = [{ label: "Auto (session default)" }]
  for (const provider of api.state.provider) {
    for (const model of Object.values(provider.models)) {
      choices.push({ label: `${provider.id}/${model.id}`, model: { providerID: provider.id, modelID: model.id } })
    }
  }
  return choices
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
