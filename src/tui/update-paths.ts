import { lstat, readFile, rename, rm, writeFile } from "node:fs/promises"
import { basename, dirname, isAbsolute, join, resolve } from "node:path"
import { KAGAN_PACKAGE, parseRelease } from "./updates"

export type UpdatePaths = {
  current: string
  prepared: string
  preparedTarget: string
  backup: string
  marker: string
}

export type UpdateMarker = { version: string }

export type FileSystem = {
  lstat: typeof lstat
  readFile: typeof readFile
  rename: typeof rename
  rm: typeof rm
  writeFile: typeof writeFile
}

export const defaultFileSystem: FileSystem = { lstat, readFile, rename, rm, writeFile }

export function wrapperTarget(wrapper: string) {
  return join(wrapper, "node_modules", "@kagan-sh", "kagan")
}

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
    packageDir !== wrapperTarget(current)
  ) {
    throw new Error("Unexpected Kagan cache layout")
  }

  const prepared = join(scopeCache, `kagan@${version}`)
  return {
    current,
    prepared,
    preparedTarget: wrapperTarget(prepared),
    backup: join(scopeCache, "kagan@latest.kagan-backup"),
    marker: join(scopeCache, "kagan@latest.kagan-update.json"),
  }
}

export async function stat(fs: FileSystem, path: string) {
  return fs.lstat(path).catch(() => undefined)
}

async function validateDirectory(fs: FileSystem, path: string) {
  const info = await fs.lstat(path)
  if (!info.isDirectory()) throw new Error(`Invalid Kagan cache path: ${path}`)
}

export async function validateWrapper(fs: FileSystem, wrapper: string, target: string, expectedVersion?: string) {
  for (const path of [wrapper, join(wrapper, "node_modules"), join(wrapper, "node_modules", "@kagan-sh"), target]) {
    await validateDirectory(fs, path)
  }

  const manifestPath = join(target, "package.json")
  const manifestInfo = await fs.lstat(manifestPath)
  if (!manifestInfo.isFile()) throw new Error("Invalid Kagan package manifest")
  const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8")) as { name?: unknown; version?: unknown }
  if (manifest.name !== KAGAN_PACKAGE || (expectedVersion !== undefined && manifest.version !== expectedVersion)) {
    throw new Error("Unexpected Kagan package in cache wrapper")
  }
}
