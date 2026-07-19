/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { BorderCharacters, Renderable, ScrollBoxRenderable } from "@opentui/core"
import { createEffect, For, Show } from "solid-js"
import { Column } from "../column"
import { COLUMNS, type ColumnType } from "../../../domain/task/types"
import type { BoardStore } from "../store"

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

export function BoardMain(props: {
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
