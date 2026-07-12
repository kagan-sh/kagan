/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { BorderCharacters, Renderable, ScrollBoxRenderable } from "@opentui/core"
import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js"
import { maybeShowOnboarding } from "../dialogs/onboarding"
import { Column } from "./column"
import { BOARD_BINDINGS, createBoardCommands, footerHints, HelpOverlay, type BoardStore } from "./commands"
import { SIDE_BORDER_CHARS } from "./borders"
import { COLUMNS, type ColumnType } from "../../domain/task/types"
import { version } from "../../../package.json"
import { useRendererDimensions } from "../renderer"
import type { UpdateStatus } from "../updates"

// Light │ so the rule stays subordinate to the heavy ┃ state bars on cards.
const COLUMN_RULE_CHARS: BorderCharacters = {
  topLeft: "",
  topRight: "",
  bottomLeft: "",
  bottomRight: "",
  horizontal: " ",
  vertical: "│",
  topT: "",
  bottomT: "",
  leftT: "",
  rightT: "",
  cross: "",
}

function Main(props: {
  api: TuiPluginApi
  store: BoardStore
  cap: number
  sendBackStopThreshold: number
  checkCommand?: string
}) {
  const theme = () => props.api.theme.current

  let hScrollRef: ScrollBoxRenderable | undefined
  const columnRefs = new Map<ColumnType, Renderable>()

  createEffect(() => {
    const column = props.store.selectedColumn()
    const scroll = hScrollRef
    const target = columnRefs.get(column)
    if (!scroll || !target) return
    const viewport = scroll.viewport
    const left = target.x - viewport.x
    const right = left + target.width
    // context: scrollBy/scrollLeft are unusable here: this scrollbox's content never measures wider than
    // its viewport (a cross-axis auto-sizing gap in the underlying ScrollBox), which pins
    // scrollWidth === viewportWidth and clamps every scrollBy to a no-op. translateX is the same
    // primitive the scrollbar itself drives (see ScrollBox's onChange), so drive it directly.
    if (right > viewport.width) scroll.content.translateX -= right - viewport.width
    if (left < 0) scroll.content.translateX -= left
  })

  return (
    <box flexGrow={1} paddingLeft={2} paddingRight={2} paddingTop={1} minHeight={0}>
      <scrollbox
        ref={(node) => {
          hScrollRef = node
        }}
        flexGrow={1}
        scrollX={true}
        scrollY={false}
        horizontalScrollbarOptions={{ visible: false }}
      >
        {/* Definite width/height (not flexGrow) is required here: this row sits inside the
            scrollbox's auto-sized content wrapper, where flexGrow can't resolve against an
            undetermined container size. Columns still overflow it horizontally via minWidth. */}
        <box width="100%" height="100%" flexDirection="row" gap={1}>
          <For each={COLUMNS}>
            {(column, index) => (
              <>
                <Show when={index() > 0}>
                  <box
                    flexShrink={0}
                    border={["left"]}
                    customBorderChars={COLUMN_RULE_CHARS}
                    borderColor={theme().border}
                  />
                </Show>
                <Column
                  api={props.api}
                  column={column}
                  cards={props.store.columns()[column]}
                  selectedID={props.store.selected()}
                  cap={column === "in_progress" ? props.cap : undefined}
                  sendBackStopThreshold={props.sendBackStopThreshold}
                  checkCommand={props.checkCommand}
                  sessionStatus={props.store.sessionStatus}
                  onSelect={(id) => props.store.select(column, id)}
                  ref={(node) => columnRefs.set(column, node)}
                />
              </>
            )}
          </For>
        </box>
      </scrollbox>
    </box>
  )
}

function updateFooter(status: Exclude<UpdateStatus, undefined>) {
  if (status.kind === "ready") return ` · v${status.version} ready — restart OpenCode`
  if (status.kind === "blocked") {
    return ` · update OpenCode to ${status.requiredOpenCode} for Kagan v${status.version}`
  }
  return " · updates unavailable"
}

function Footer(props: { api: TuiPluginApi; store: BoardStore; hints: () => { key: string; label: string }[] }) {
  const theme = () => props.api.theme.current
  const filter = () => props.store.filter()

  return (
    <box flexDirection="row" flexShrink={0} paddingLeft={2} paddingRight={2} justifyContent="space-between">
      <text wrapMode="none" truncate={true} fg={theme().textMuted}>
        kagan v{version}
        <Show when={props.store.updateStatus()}>
          {(status) => (
            <span style={{ fg: status().kind === "ready" ? theme().info : theme().warning }}>
              {updateFooter(status())}
            </span>
          )}
        </Show>
        <Show when={filter()}>{` · filter: ${filter()}`}</Show>
      </text>
      <box flexDirection="row" gap={2} flexShrink={0}>
        <For each={props.hints()}>
          {(hint) => (
            <text wrapMode="none" truncate={true}>
              <span style={{ fg: theme().text }}>{hint.key}</span>
              <span style={{ fg: theme().textMuted }}> {hint.label}</span>
            </text>
          )}
        </For>
      </box>
    </box>
  )
}

// api.ui.toast doesn't render on plugin routes — see the Notice rationale in store.ts.
function Notice(props: { api: TuiPluginApi; store: BoardStore }) {
  const theme = () => props.api.theme.current
  const dimensions = useRendererDimensions(props.api)

  return (
    <box position="absolute" top={2} right={2} flexDirection="column" alignItems="flex-end" gap={1} zIndex={2}>
      <For each={props.store.notices()}>
        {(notice) => (
          <box
            maxWidth={Math.min(60, dimensions().width - 6)}
            paddingLeft={2}
            paddingRight={2}
            paddingTop={1}
            paddingBottom={1}
            border={["left", "right"]}
            customBorderChars={SIDE_BORDER_CHARS}
            borderColor={theme()[notice.variant ?? "info"]}
            backgroundColor={theme().backgroundPanel}
          >
            <text fg={theme().text} wrapMode="word">
              {notice.message}
            </text>
          </box>
        )}
      </For>
    </box>
  )
}

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
          <Main
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

function keymapKey(key: string): string | { name: string } {
  if (key === ",") return { name: "," }
  if (key === "?") return "?,shift+/"
  if (key === "return") return "return,enter"
  return key
}
