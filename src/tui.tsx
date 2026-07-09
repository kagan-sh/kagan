/** @jsxImportSource @opentui/solid */
import type { TuiPlugin, TuiPluginApi, TuiPluginModule } from "@opencode-ai/plugin/tui"
import { createRoot } from "solid-js"
import { Board } from "./tui/board/board"
import { Settings } from "./tui/routes/settings"
import { showOnboarding } from "./tui/dialogs/onboarding"
import { createBoardStore, createSessionEventSubscription, createSessionStatusSubscription } from "./tui/board/store"
import { checkForUpdate, type UpdateStatus } from "./tui/updates"
import { cleanupPreparedUpdate, prepareUpdate } from "./tui/update-manager"
import { ROUTE, SETTINGS_ROUTE } from "./tui/types"
import { version } from "../package.json"

export function showUpdateToast(api: TuiPluginApi, currentVersion: string, status: Exclude<UpdateStatus, undefined>) {
  if (api.route.current.name !== "home" && api.route.current.name !== "session") return
  api.ui.toast({
    variant: status.kind === "ready" ? "success" : "warning",
    title: "Kagan",
    message:
      status.kind === "ready"
        ? `Kagan v${status.version} is ready. Restart OpenCode to apply.`
        : `Kagan v${currentVersion} remains active. Kagan v${status.version} requires OpenCode ${status.requiredOpenCode}.`,
  })
}

const tui: TuiPlugin = async (api, options, meta) => {
  const store = createRoot(() => createBoardStore(api, options))
  const disposeEvents = createSessionEventSubscription(api, () => store.refresh())
  const disposeStatusEvents = createSessionStatusSubscription(api, store.setSessionStatus)
  api.lifecycle.onDispose(() => disposeEvents())
  api.lifecycle.onDispose(() => disposeStatusEvents())

  cleanupPreparedUpdate(meta, version)
    .catch(() => {})
    .then(() =>
      checkForUpdate({
        kv: api.kv,
        currentVersion: version,
        openCodeVersion: api.app.version,
        source: meta.source,
        spec: meta.spec,
        now: Date.now(),
      }),
    )
    .then(async (status) => {
      if (!status || api.lifecycle.signal.aborted) return
      if (status.kind === "ready" && !(await prepareUpdate({ api, meta, currentVersion: version, status }))) return
      store.setUpdateStatus(status)
      showUpdateToast(api, version, status)
    })
    .catch(() => {})

  api.route.register([
    {
      name: ROUTE,
      render: () => <Board api={api} store={store} />,
    },
    {
      name: SETTINGS_ROUTE,
      render: () => <Settings api={api} options={options} />,
    },
  ])

  api.keymap.registerLayer({
    commands: [
      {
        name: "kagan.open",
        title: "Open Kagan",
        category: "Kagan",
        namespace: "palette",
        slashName: "kagan",
        run() {
          api.route.navigate(ROUTE)
          store.refresh()
        },
      },
      {
        name: "kagan.settings",
        title: "Open Kagan settings",
        category: "Kagan",
        namespace: "palette",
        slashName: "kagan-settings",
        run() {
          api.route.navigate(SETTINGS_ROUTE)
        },
      },
      {
        name: "kagan.tutorial",
        title: "Show the Kagan tutorial",
        category: "Kagan",
        namespace: "palette",
        slashName: "kagan-tutorial",
        run() {
          if (api.route.current.name !== ROUTE) {
            api.route.navigate(ROUTE)
            store.refresh()
          }
          showOnboarding(api)
        },
      },
    ],
    // "leader" is the host TUI's registered leader token (default ctrl+x); the shared
    // keymap resolves it even when this layer registers before the token exists.
    bindings: [{ key: "<leader>k", cmd: "kagan.open", desc: "Open Kagan" }],
  })
}

const plugin: TuiPluginModule = {
  id: "kagan",
  tui,
}

export default plugin
