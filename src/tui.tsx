/** @jsxImportSource @opentui/solid */
import type { TuiPlugin, TuiPluginModule } from "@opencode-ai/plugin/tui"
import { createRoot } from "solid-js"
import { Board } from "./board"
import { Settings } from "./settings"
import { showOnboarding } from "./onboarding"
import { createBoardStore, createSessionEventSubscription, createSessionStatusSubscription } from "./store"
import { ROUTE, SETTINGS_ROUTE } from "./types"

const tui: TuiPlugin = async (api, options) => {
  const store = createRoot(() => createBoardStore(api, options))
  const disposeEvents = createSessionEventSubscription(api, () => store.refresh())
  const disposeStatusEvents = createSessionStatusSubscription(api, store.setSessionStatus)
  api.lifecycle.onDispose(() => disposeEvents())
  api.lifecycle.onDispose(() => disposeStatusEvents())

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
