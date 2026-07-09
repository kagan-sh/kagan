import { lstat, readFile, realpath, rename, rm, writeFile } from "node:fs/promises"
import { basename, dirname, isAbsolute, join, resolve } from "node:path"
import { KAGAN_PACKAGE, parseRelease } from "./updates"

export type UpdatePaths = {
  current: string
  prepared: string
  preparedTarget: string
  backup: string
  marker: string
}

export type UpdateMarker = Pick<UpdatePaths, "current" | "prepared" | "backup"> & { version: string }

export type FileSystem = {
  lstat: typeof lstat
  readFile: typeof readFile
  realpath: typeof realpath
  rename: typeof rename
  rm: typeof rm
  writeFile: typeof writeFile
}

export const defaultFileSystem: FileSystem = { lstat, readFile, realpath, rename, rm, writeFile }

export function updatePaths(target: string, version: string): UpdatePaths {
  if (!isAbsolute(target) || resolve(target) !== target || !parseRelease(version)) {
    throw new Error("Invalid Kagan update path")
  }

  const packageDir = target
  const packageScope = dirname(packageDir)
  const nodeModules = dirname(packageScope)
  const current = dirname(nodeModules)
  const scopeCache = dirname(current)
  const packagesCache = dirname(scopeCache)
  const openCodeCache = dirname(packagesCache)
  if (
    basename(packageDir) !== "kagan" ||
    basename(packageScope) !== "@kagan-sh" ||
    basename(nodeModules) !== "node_modules" ||
    basename(current) !== "kagan@latest" ||
    basename(scopeCache) !== "@kagan-sh" ||
    basename(packagesCache) !== "packages" ||
    basename(openCodeCache) !== "opencode" ||
    packageDir !== join(current, "node_modules", "@kagan-sh", "kagan")
  ) {
    throw new Error("Unexpected Kagan cache layout")
  }

  const prepared = join(scopeCache, `kagan@${version}`)
  return {
    current,
    prepared,
    preparedTarget: join(prepared, "node_modules", "@kagan-sh", "kagan"),
    backup: join(scopeCache, "kagan@latest.kagan-backup"),
    marker: join(scopeCache, "kagan@latest.kagan-update.json"),
  }
}

export async function stat(fs: FileSystem, path: string) {
  return fs.lstat(path).catch(() => undefined)
}

async function validateDirectory(fs: FileSystem, path: string) {
  const info = await fs.lstat(path)
  if (!info.isDirectory() || info.isSymbolicLink()) throw new Error(`Unsafe Kagan cache path: ${path}`)
}

export function validMarkerFile(info: Awaited<ReturnType<FileSystem["lstat"]>> | undefined) {
  return info?.isFile() === true && !info.isSymbolicLink() && info.nlink === 1
}

export async function validateWrapper(fs: FileSystem, wrapper: string, target: string, expectedVersion?: string) {
  const scopeCache = dirname(wrapper)
  const packagesCache = dirname(scopeCache)
  const openCodeCache = dirname(packagesCache)
  for (const path of [
    openCodeCache,
    packagesCache,
    scopeCache,
    wrapper,
    join(wrapper, "node_modules"),
    join(wrapper, "node_modules", "@kagan-sh"),
    target,
  ]) {
    await validateDirectory(fs, path)
  }

  const [wrapperReal, targetReal] = await Promise.all([fs.realpath(wrapper), fs.realpath(target)])
  if (targetReal !== join(wrapperReal, "node_modules", "@kagan-sh", "kagan")) {
    throw new Error("Kagan cache wrapper escapes its expected directory")
  }

  const manifestPath = join(target, "package.json")
  const manifestInfo = await fs.lstat(manifestPath)
  if (!manifestInfo.isFile() || manifestInfo.isSymbolicLink()) throw new Error("Unsafe Kagan package manifest")
  const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8")) as { name?: unknown; version?: unknown }
  if (manifest.name !== KAGAN_PACKAGE || (expectedVersion !== undefined && manifest.version !== expectedVersion)) {
    throw new Error("Unexpected Kagan package in cache wrapper")
  }
}

export function markerMatches(marker: UpdateMarker, paths: UpdatePaths, version: string) {
  return (
    marker.version === version &&
    marker.current === paths.current &&
    marker.prepared === paths.prepared &&
    marker.backup === paths.backup
  )
}
