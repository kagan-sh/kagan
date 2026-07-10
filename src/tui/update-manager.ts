import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { join } from "node:path"
import { cleanupPreparedUpdate } from "./update-cleanup"
import { checkForUpdate, isAutomaticUpdateInstall, KAGAN_PACKAGE, parseRelease, type UpdateStatus } from "./updates"
import {
  defaultFileSystem,
  type FileSystem,
  type UpdateMarker,
  type UpdatePaths,
  updatePaths,
  validateWrapper,
  stat,
} from "./update-paths"

async function promotePreparedUpdate(
  paths: UpdatePaths,
  currentVersion: string,
  preparedVersion: string,
  fs: FileSystem = defaultFileSystem,
): Promise<void> {
  await validateWrapper(fs, paths.current, join(paths.current, "node_modules", "@kagan-sh", "kagan"), currentVersion)
  await validateWrapper(fs, paths.prepared, paths.preparedTarget, preparedVersion)
  if (await stat(fs, paths.backup)) throw new Error("Kagan update backup already exists")
  await fs.rename(paths.current, paths.backup)
  try {
    await fs.rename(paths.prepared, paths.current)
  } catch (error) {
    try {
      await fs.rename(paths.backup, paths.current)
    } catch (restoreError) {
      throw new AggregateError([error, restoreError], "Kagan update promotion and restore failed")
    }
    throw error
  }
}

export async function prepareUpdate(input: {
  api: TuiPluginApi
  meta: TuiPluginMeta
  currentVersion: string
  status: UpdateStatus
  fs?: FileSystem
}): Promise<boolean> {
  const { api, meta, currentVersion, status } = input
  if (
    status?.kind !== "ready" ||
    !isAutomaticUpdateInstall({ source: meta.source, spec: meta.spec, version: currentVersion }) ||
    !parseRelease(status.version)
  ) {
    return false
  }

  // Host dedupes plugins.add by module id so this import does not activate a second kagan instance.
  const installed = await api.plugins.add(`${KAGAN_PACKAGE}@${status.version}`).catch(() => false)
  if (!installed) return false

  const fs = input.fs ?? defaultFileSystem
  try {
    const paths = updatePaths(meta.target, status.version)
    await validateWrapper(fs, paths.current, meta.target, currentVersion)
    await validateWrapper(fs, paths.prepared, paths.preparedTarget, status.version)
    if (api.lifecycle.signal.aborted) return false

    const marker: UpdateMarker = {
      version: status.version,
      current: paths.current,
      prepared: paths.prepared,
      backup: paths.backup,
    }
    await fs.writeFile(paths.marker, JSON.stringify(marker))
    if (api.lifecycle.signal.aborted) return false
    api.lifecycle.onDispose(() => promotePreparedUpdate(paths, currentVersion, status.version, fs))
    return true
  } catch {
    return false
  }
}

export { cleanupPreparedUpdate } from "./update-cleanup"
