/** @jsxImportSource @opentui/solid */
import { createSignal, onCleanup } from "solid-js"
import type { CardDisplayProps } from "./card/shell-props"
import { CardShell } from "./card/body"

export function Card(props: CardDisplayProps) {
  const [renderedAt] = createSignal(Date.now())
  onCleanup(() => props.onCardRef?.(props.session.id, undefined))

  return <CardShell {...props} renderedAt={renderedAt()} />
}
