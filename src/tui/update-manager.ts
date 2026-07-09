import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { join } from "node:path"
import { isAutomaticUpdateInstall, KAGAN_PACKAGE, parseRelease, type UpdateStatus } from "./updates"
import {
  defaultFileSystem,
  type FileSystem,
  markerMatches,
  stat,
  type UpdateMarker,
  type UpdatePaths,
  updatePaths,
  validateWrapper,
  validMarkerFile,
} from "./update-paths"

export async function cleanupPreparedUpdate(
  meta: TuiPluginMeta,
  currentVersion: string,
  fs: FileSystem = defaultFileSystem,
): Promise<void> {
  if (!isAutomaticUpdateInstall({ source: meta.source, spec: meta.spec, version: currentVersion })) return
  const paths = updatePaths(meta.target, currentVersion)
  const markerInfo = await stat(fs, paths.marker)
  if (!markerInfo) return
  if (!validMarkerFile(markerInfo)) throw new Error("Unsafe Kagan update marker")

  const marker = JSON.parse(await fs.readFile(paths.marker, "utf8")) as UpdateMarker
  if (!markerMatches(marker, paths, currentVersion)) return
  await validateWrapper(fs, paths.current, meta.target, currentVersion)

  const backupInfo = await stat(fs, paths.backup)
  if (backupInfo) {
    await validateWrapper(fs, paths.backup, join(paths.backup, "node_modules", "@kagan-sh", "kagan"))
    await fs.rm(paths.backup, { recursive: true })
  }
  if (!validMarkerFile(await stat(fs, paths.marker))) throw new Error("Unsafe Kagan update marker")
  await fs.rm(paths.marker)
}

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

  const installed = await api.plugins.add(`${KAGAN_PACKAGE}@${status.version}`).catch(() => false)
  if (!installed) return false

  const fs = input.fs ?? defaultFileSystem
  try {
    const paths = updatePaths(meta.target, status.version)
    await validateWrapper(fs, paths.current, meta.target, currentVersion)
    await validateWrapper(fs, paths.prepared, paths.preparedTarget, status.version)
    if (await stat(fs, paths.backup)) return false
    const markerInfo = await stat(fs, paths.marker)
    if (markerInfo && !validMarkerFile(markerInfo)) return false
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
