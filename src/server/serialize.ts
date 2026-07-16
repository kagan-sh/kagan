// Serializes async work by key so overlapping bulk task-creation runs can't read the same session
// snapshot and mint duplicate task numbers. Scoped to this server process.
const tails = new Map<string, Promise<unknown>>()

export function serializeByKey<T>(key: string, task: () => Promise<T>): Promise<T> {
  const prior = tails.get(key) ?? Promise.resolve()
  const run = prior.then(task, task)
  const settled = run.then(
    () => undefined,
    () => undefined,
  )
  tails.set(key, settled)
  void settled.then(() => {
    if (tails.get(key) === settled) tails.delete(key)
  })
  return run
}
