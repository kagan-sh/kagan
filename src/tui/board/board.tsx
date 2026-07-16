/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js"
import { maybeShowOnboarding } from "../dialogs/onboarding"
import { kagan } from "../../domain/task/metadata"
import { boardBindings, createBoardCommands, footerHints, HelpOverlay, type BoardStore } from "./commands"
import { keymapKey } from "./layout/keymap"
import { BoardMain } from "./layout/main"
import { Footer } from "./layout/footer"
import { Notice } from "./layout/notice"

export function Board(props: { api: TuiPluginApi; store: BoardStore }) {
  const store = props.store
  const [helpOpen, setHelpOpen] = createSignal(false)
  const commands = createBoardCommands(props.api, store, setHelpOpen)

  let disposeLayer: (() => void) | undefined

  onMount(() => maybeShowOnboarding(props.api))

  const updateAvailable = createMemo(() => store.updateStatus()?.kind === "available")

  createEffect(() => {
    const bindings = boardBindings(updateAvailable()).map((binding) => ({
      key: keymapKey(binding.key),
      cmd: binding.cmd,
      desc: binding.desc,
    }))
    disposeLayer?.()
    disposeLayer = props.api.keymap.registerLayer({
      mode: "base",
      priority: 100,
      commands,
      bindings,
    })
  })

  onCleanup(() => disposeLayer?.())

  const waitingPermissions = createMemo(() =>
    store
      .sessions()
      .filter((session) => !session.parentID)
      .reduce((sum, session) => sum + (kagan(session.metadata).awaitingPermissions?.length ?? 0), 0),
  )
  const hints = createMemo(() =>
    footerHints(store.selectedSession(), store.filter() !== "", waitingPermissions(), updateAvailable()),
  )
  return (
    <box position="absolute" left={0} top={0} width="100%" height="100%">
      <box flexDirection="column" width="100%" height="100%">
        <box flexGrow={1} minHeight={0}>
          <BoardMain
            api={props.api}
            store={store}
            cap={store.inProgressCap}
            sendBackStopThreshold={store.sendBackStopThreshold}
            checkCommand={store.checkCommand}
          />
        </box>
        <Footer api={props.api} store={store} hints={hints} />
      </box>
      <HelpOverlay api={props.api} visible={() => helpOpen()} updateAvailable={updateAvailable} />
      <Notice api={props.api} store={store} />
    </box>
  )
}
