import type { TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { basename, dirname, join } from "node:path"
import { isAutomaticUpdateInstall, parseRelease } from "./updates"
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

// Guard against a corrupted marker naming an arbitrary path: only sweep prepared dirs Kagan itself
// could have created — a `kagan@<x.y.z>` sibling of the current wrapper in the same scope cache.
function preparedInCache(preparedPath: string, paths: UpdatePaths): boolean {
  if (dirname(preparedPath) !== dirname(paths.current)) return false
  const name = basename(preparedPath)
  if (!name.startsWith("kagan@")) return false
  return parseRelease(name.slice("kagan@".length)) !== undefined
}

async function removeStaleMarker(fs: FileSystem, paths: UpdatePaths, marker: UpdateMarker) {
  if (marker.prepared && preparedInCache(marker.prepared, paths)) {
    await fs.rm(marker.prepared, { recursive: true, force: true }).catch(() => {})
  }
  await fs.rm(paths.marker, { force: true })
}

export async function cleanupPreparedUpdate(
  meta: TuiPluginMeta,
  currentVersion: string,
  fs: FileSystem = defaultFileSystem,
): Promise<void> {
  if (!isAutomaticUpdateInstall({ source: meta.source, spec: meta.spec, version: currentVersion })) return
  const paths = updatePaths(meta.target, currentVersion)
  const marker = await readMarker(fs, paths.marker)
  const matches = Boolean(marker && markerMatches(marker, paths, currentVersion))

  await removeOrphanBackup(fs, paths, meta.target, currentVersion, marker, matches)

  if (marker && !matches) {
    await removeStaleMarker(fs, paths, marker)
  }

  const markerForCleanup = await readMarker(fs, paths.marker)
  if (!markerForCleanup || !markerMatches(markerForCleanup, paths, currentVersion)) return

  await validateWrapper(fs, paths.current, meta.target, currentVersion)
  if (await stat(fs, paths.backup)) {
    await validateWrapper(fs, paths.backup, join(paths.backup, "node_modules", "@kagan-sh", "kagan"))
    await fs.rm(paths.backup, { recursive: true })
  }
  // Interrupted promotion + host re-download leaves the prepared dir behind; it is self-computed, so sweep it.
  if (await stat(fs, paths.prepared)) await fs.rm(paths.prepared, { recursive: true })
  await fs.rm(paths.marker)
}
