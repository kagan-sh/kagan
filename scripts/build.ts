import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises"
import { join, relative, resolve } from "node:path"
import { transformAsync } from "@babel/core"
import { runtimeModuleIdForSpecifier } from "@opentui/core/runtime-plugin"
// @ts-expect-error untyped preset
import solidPreset from "babel-preset-solid"
// @ts-expect-error untyped preset
import tsPreset from "@babel/preset-typescript"
// @ts-expect-error untyped plugin
import moduleResolver from "babel-plugin-module-resolver"

const repoRoot = resolve(import.meta.dir, "..")
const srcDir = join(repoRoot, "src")
const distDir = join(repoRoot, "dist")
const hostRuntimeSpecifiers = new Set([
  "@opentui/core",
  "@opentui/core/testing",
  "@opentui/keymap",
  "@opentui/keymap/extras",
  "@opentui/keymap/extras/graph",
  "@opentui/keymap/addons",
  "@opentui/keymap/addons/opentui",
  "@opentui/keymap/html",
  "@opentui/keymap/opentui",
  "@opentui/keymap/react",
  "@opentui/keymap/solid",
  "@opentui/solid",
  "@opentui/solid/components",
  "@opentui/solid/jsx-runtime",
  "@opentui/solid/jsx-dev-runtime",
  "solid-js",
  "solid-js/store",
])

function resolveImportPath(specifier: string): string | null {
  if (hostRuntimeSpecifiers.has(specifier)) return runtimeModuleIdForSpecifier(specifier)
  if (!specifier.startsWith(".")) return null
  if (/\.(json|js|mjs|cjs)$/.test(specifier)) return specifier
  if (/\.tsx?$/.test(specifier)) return specifier.replace(/\.tsx?$/, ".js")
  return `${specifier}.js`
}

async function transformSolidSource(code: string, filename: string): Promise<string> {
  const cleanFilename = filename.replace(/[?#].*$/, "")
  const presets: unknown[] = []
  if (/\.[cm]?[jt]sx$/.test(cleanFilename)) {
    presets.push([solidPreset, { moduleName: runtimeModuleIdForSpecifier("@opentui/solid"), generate: "universal" }])
  }
  if (/\.[cm]?tsx?$/.test(cleanFilename)) {
    presets.push([tsPreset])
  }
  const plugins = [[moduleResolver, { resolvePath: (specifier: string) => resolveImportPath(specifier) ?? specifier }]]
  const result = await transformAsync(code, {
    filename: cleanFilename,
    configFile: false,
    babelrc: false,
    presets,
    plugins,
  })
  return result?.code ?? code
}

async function listSourceFiles(): Promise<string[]> {
  const entries = await readdir(srcDir, { withFileTypes: true })
  return entries
    .filter((entry) => entry.isFile() && /\.tsx?$/.test(entry.name))
    .map((entry) => join(srcDir, entry.name))
    .sort()
}

await rm(distDir, { recursive: true, force: true })
await mkdir(distDir, { recursive: true })

const files = await listSourceFiles()
for (const file of files) {
  const rel = relative(srcDir, file)
  const outPath = join(distDir, rel.replace(/\.tsx?$/, ".js"))
  const code = await readFile(file, "utf8")
  const transformed = await transformSolidSource(code, rel)
  await writeFile(outPath, transformed)
}

console.error(`built ${files.length} files to dist/`)
