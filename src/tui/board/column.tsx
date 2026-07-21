/** @jsxImportSource @opentui/solid */
import { For, createEffect, createSignal, onCleanup } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Renderable, ScrollBoxRenderable } from "@opentui/core"
import { TextAttributes } from "@opentui/core"
import type { SessionStatus } from "@opencode-ai/sdk/v2"
import { CardShell, type CardDisplayProps } from "./card/body"
import type { ColumnType } from "../../domain/task/types"
import type { BoardCard } from "../types"
import type { BoardStore } from "./store"

function columnLabel(column: ColumnType): string {
  if (column === "in_progress") return "In Progress"
  return column.charAt(0).toUpperCase() + column.slice(1)
}

function Card(props: CardDisplayProps) {
  const [renderedAt] = createSignal(Date.now())
  onCleanup(() => props.onCardRef?.(props.session.id, undefined))

  return <CardShell {...props} renderedAt={renderedAt()} />
}

export function Column(props: {
  api: TuiPluginApi
  store: BoardStore
  column: ColumnType
  cards: BoardCard[]
  selectedID?: string
  onSelect: (id: string) => void
  sessionStatus?: (id: string) => SessionStatus["type"] | undefined
  ref?: (node: Renderable) => void
}) {
  const theme = () => props.api.theme.current
  const cap = () => (props.column === "in_progress" ? props.store.inProgressCap : undefined)
  const atCap = () => {
    const limit = cap()
    return limit !== undefined && props.cards.length >= limit
  }
  const countText = () => {
    const limit = cap()
    const raw = limit !== undefined ? `${props.cards.length}/${limit}` : `${props.cards.length}`
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
              store={props.store}
              session={boardCard.session}
              children={boardCard.children}
              selectedID={props.selectedID}
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
