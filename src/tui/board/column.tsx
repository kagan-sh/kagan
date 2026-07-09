/** @jsxImportSource @opentui/solid */
import { For, createEffect } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Renderable, ScrollBoxRenderable } from "@opentui/core"
import { TextAttributes } from "@opentui/core"
import type { SessionStatus } from "@opencode-ai/sdk/v2"
import { Card } from "./card"
import type { ColumnType } from "../../domain/task/types"
import type { BoardCard } from "../types"

function columnLabel(column: ColumnType): string {
  if (column === "in_progress") return "In Progress"
  return column.charAt(0).toUpperCase() + column.slice(1)
}

export function Column(props: {
  api: TuiPluginApi
  column: ColumnType
  cards: BoardCard[]
  selectedID?: string
  onSelect: (id: string) => void
  cap?: number
  sendBackStopThreshold?: number
  checkCommand?: string
  sessionStatus?: (id: string) => SessionStatus["type"] | undefined
  ref?: (node: Renderable) => void
}) {
  const theme = () => props.api.theme.current
  const atCap = () => props.cap !== undefined && props.cards.length >= props.cap
  const countText = () => {
    const raw = props.cap !== undefined ? `${props.cards.length}/${props.cap}` : `${props.cards.length}`
    return atCap() ? `△ ${raw}` : raw
  }
  const headerColor = () => (atCap() ? theme().warning : theme().text)

  let scrollRef: ScrollBoxRenderable | undefined
  const cardRefs = new Map<string, Renderable>()
  const registerCardRef = (id: string, node: Renderable | undefined) => {
    if (node) cardRefs.set(id, node)
    else cardRefs.delete(id)
  }

  createEffect(() => {
    const id = props.selectedID
    const scroll = scrollRef
    const target = id ? cardRefs.get(id) : undefined
    if (!scroll || !target) return
    const viewport = scroll.viewport
    const top = target.y - viewport.y
    const bottom = top + target.height
    if (bottom > viewport.height) scroll.scrollBy(bottom - viewport.height)
    if (top < 0) scroll.scrollBy(top)
  })

  return (
    <box ref={props.ref} flexGrow={1} flexDirection="column" minWidth={20} minHeight={0} overflow="hidden">
      <box flexDirection="row" gap={1} paddingBottom={1}>
        <text
          flexGrow={1}
          flexShrink={1}
          wrapMode="none"
          truncate={true}
          fg={headerColor()}
          attributes={TextAttributes.BOLD}
        >
          {columnLabel(props.column)}
        </text>
        <text flexShrink={0} wrapMode="none" truncate={true} fg={headerColor()}>
          {countText()}
        </text>
      </box>
      <scrollbox
        ref={(node) => {
          scrollRef = node
        }}
        flexGrow={1}
        minHeight={0}
        gap={1}
        verticalScrollbarOptions={{ visible: false }}
      >
        <For each={props.cards}>
          {(boardCard) => (
            <Card
              api={props.api}
              session={boardCard.session}
              children={boardCard.children}
              selectedID={props.selectedID}
              sendBackStopThreshold={props.sendBackStopThreshold}
              checkCommand={props.checkCommand}
              sessionStatus={props.sessionStatus}
              onSelect={props.onSelect}
              onCardRef={registerCardRef}
            />
          )}
        </For>
      </scrollbox>
    </box>
  )
}
