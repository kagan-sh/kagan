import type { TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { dirname, join } from "node:path"
import { isAutomaticUpdateInstall, parseRelease } from "./updates"
import {
  defaultFileSystem,
  type FileSystem,
  stat,
  type UpdateMarker,
  type UpdatePaths,
  updatePaths,
  validateWrapper,
  wrapperTarget,
} from "./update-paths"

async function readMarker(fs: FileSystem, markerPath: string): Promise<UpdateMarker | undefined> {
  const info = await stat(fs, markerPath)
  if (!info?.isFile()) return
  try {
    const marker = JSON.parse(await fs.readFile(markerPath, "utf8")) as { version?: unknown }
    if (typeof marker.version !== "string") throw new Error("Invalid Kagan update marker")
    return { version: marker.version }
  } catch {
    await fs.rm(markerPath, { force: true })
  }
}

async function removeOrphanBackup(
  fs: FileSystem,
  paths: UpdatePaths,
  target: string,
  currentVersion: string,
  matchesCurrent: boolean,
) {
  if (!(await stat(fs, paths.backup))) return
  if (matchesCurrent) return
  await validateWrapper(fs, paths.current, target, currentVersion)
  await fs.rm(paths.backup, { recursive: true })
}

async function removeStaleMarker(fs: FileSystem, paths: UpdatePaths, marker: UpdateMarker) {
  if (parseRelease(marker.version)) {
    await fs
      .rm(join(dirname(paths.current), `kagan@${marker.version}`), { recursive: true, force: true })
      .catch(() => {})
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
  const matches = marker?.version === currentVersion

  await removeOrphanBackup(fs, paths, meta.target, currentVersion, matches)

  if (marker && !matches) {
    await removeStaleMarker(fs, paths, marker)
  }

  if (!matches) return

  await validateWrapper(fs, paths.current, meta.target, currentVersion)
  if (await stat(fs, paths.backup)) {
    await validateWrapper(fs, paths.backup, wrapperTarget(paths.backup))
    await fs.rm(paths.backup, { recursive: true })
  }
  // Interrupted promotion + host re-download leaves the prepared dir behind; it is self-computed, so sweep it.
  if (await stat(fs, paths.prepared)) await fs.rm(paths.prepared, { recursive: true })
  await fs.rm(paths.marker)
}
