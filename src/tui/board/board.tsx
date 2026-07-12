/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createMemo, createSignal, onCleanup, onMount } from "solid-js"
import { maybeShowOnboarding } from "../dialogs/onboarding"
import { BOARD_BINDINGS, createBoardCommands, footerHints, HelpOverlay, type BoardStore } from "./commands"
import { keymapKey } from "./layout/keymap"
import { BoardMain } from "./layout/main"
import { Footer } from "./layout/footer"
import { Notice } from "./layout/notice"

export function Board(props: { api: TuiPluginApi; store: BoardStore }) {
  const store = props.store
  const [helpOpen, setHelpOpen] = createSignal(false)
  const commands = createBoardCommands(props.api, store, setHelpOpen)

  let disposeLayer: (() => void) | undefined

  onMount(() => {
    maybeShowOnboarding(props.api)
    disposeLayer = props.api.keymap.registerLayer({
      mode: "base",
      priority: 100,
      commands,
      bindings: BOARD_BINDINGS.map((binding) => ({
        key: keymapKey(binding.key),
        cmd: binding.cmd,
        desc: binding.desc,
      })),
    })
  })

  onCleanup(() => disposeLayer?.())

  const hints = createMemo(() => footerHints(store.selectedSession(), store.filter() !== ""))
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
      <HelpOverlay api={props.api} visible={() => helpOpen()} />
      <Notice api={props.api} store={store} />
    </box>
  )
}
