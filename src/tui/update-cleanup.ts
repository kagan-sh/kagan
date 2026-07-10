import type { TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { join } from "node:path"
import { isAutomaticUpdateInstall } from "./updates"
import {
  defaultFileSystem,
  type FileSystem,
  markerMatches,
  stat,
  type UpdateMarker,
  type UpdatePaths,
  updatePaths,
  validateWrapper,
} from "./update-paths"

async function readMarker(fs: FileSystem, markerPath: string): Promise<UpdateMarker | undefined> {
  const info = await stat(fs, markerPath)
  if (!info?.isFile()) return
  try {
    return JSON.parse(await fs.readFile(markerPath, "utf8")) as UpdateMarker
  } catch {
    await fs.rm(markerPath, { force: true })
  }
}

async function removeOrphanBackup(
  fs: FileSystem,
  paths: UpdatePaths,
  target: string,
  currentVersion: string,
  marker: UpdateMarker | undefined,
  matchesCurrent: boolean,
) {
  if (!(await stat(fs, paths.backup))) return
  if (marker && matchesCurrent) return
  await validateWrapper(fs, paths.current, target, currentVersion)
  await fs.rm(paths.backup, { recursive: true })
}

async function removeStaleMarker(fs: FileSystem, markerPath: string, marker: UpdateMarker) {
  if (marker.prepared) await fs.rm(marker.prepared, { recursive: true, force: true }).catch(() => {})
  await fs.rm(markerPath, { force: true })
}

async function restoreCurrentFromBackup(fs: FileSystem, paths: UpdatePaths, currentVersion: string) {
  if (await stat(fs, paths.current)) return
  if (!(await stat(fs, paths.backup))) return
  await validateWrapper(fs, paths.backup, join(paths.backup, "node_modules", "@kagan-sh", "kagan"), currentVersion)
  await fs.rename(paths.backup, paths.current)
}

export async function cleanupPreparedUpdate(
  meta: TuiPluginMeta,
  currentVersion: string,
  fs: FileSystem = defaultFileSystem,
): Promise<void> {
  if (!isAutomaticUpdateInstall({ source: meta.source, spec: meta.spec, version: currentVersion })) return
  const paths = updatePaths(meta.target, currentVersion)
  const marker = await readMarker(fs, paths.marker)
  const interruptedPromotion = Boolean(
    marker && marker.current === paths.current && !(await stat(fs, paths.current)) && (await stat(fs, paths.backup)),
  )

  if (interruptedPromotion) {
    await restoreCurrentFromBackup(fs, paths, currentVersion)
  }

  const markerAfterRestore = await readMarker(fs, paths.marker)
  const matchesAfterRestore = Boolean(markerAfterRestore && markerMatches(markerAfterRestore, paths, currentVersion))

  await removeOrphanBackup(fs, paths, meta.target, currentVersion, markerAfterRestore, matchesAfterRestore)

  if (markerAfterRestore && !matchesAfterRestore) {
    await removeStaleMarker(fs, paths.marker, markerAfterRestore)
  }

  const markerForCleanup = await readMarker(fs, paths.marker)
  if (!markerForCleanup || !markerMatches(markerForCleanup, paths, currentVersion)) return

  await validateWrapper(fs, paths.current, meta.target, currentVersion)
  if (await stat(fs, paths.backup)) {
    await validateWrapper(fs, paths.backup, join(paths.backup, "node_modules", "@kagan-sh", "kagan"))
    await fs.rm(paths.backup, { recursive: true })
  }
  await fs.rm(paths.marker)
}
