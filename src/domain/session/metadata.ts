import { AsyncLocalStorage } from "node:async_hooks"

export function mergeKagan(
  current: Record<string, unknown>,
  partial: Record<string, unknown>,
): Record<string, unknown> {
  const existing = current.kagan
  return { ...current, kagan: { ...(typeof existing === "object" && existing !== null ? existing : {}), ...partial } }
}

const locks = ((globalThis as Record<string, unknown>).__kaganSessionLocks ??= new Map<
  string,
  Promise<unknown>
>()) as Map<string, Promise<unknown>>
const heldLocks = new AsyncLocalStorage<ReadonlySet<string>>()

export function lockSessionMetadata<T>(sessionID: string, fn: () => Promise<T>): Promise<T> {
  const held = heldLocks.getStore()
  if (held?.has(sessionID)) return fn()
  const previous = locks.get(sessionID) ?? Promise.resolve()
  const nextHeld = new Set(held)
  nextHeld.add(sessionID)
  const result = previous.then(() => heldLocks.run(nextHeld, fn))
  const tail = result.then(
    () => undefined,
    () => undefined,
  )
  locks.set(sessionID, tail)
  void tail.then(() => {
    if (locks.get(sessionID) === tail) locks.delete(sessionID)
  })
  return result
}
