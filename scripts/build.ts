import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises"
import { join, relative, resolve } from "node:path"
import { transformSolidSource } from "../node_modules/@opentui/solid/scripts/solid-transform.js"

const repoRoot = resolve(import.meta.dir, "..")
const srcDir = join(repoRoot, "src")
const distDir = join(repoRoot, "dist")

function resolveImportPath(specifier: string): string | null {
  if (!specifier.startsWith(".")) return null
  if (/\.(json|js|mjs|cjs)$/.test(specifier)) return specifier
  if (/\.tsx?$/.test(specifier)) return specifier.replace(/\.tsx?$/, ".js")
  return `${specifier}.js`
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
  const transformed = await transformSolidSource(code, {
    filename: rel,
    resolvePath: (specifier) => resolveImportPath(specifier) ?? specifier,
  })
  await writeFile(outPath, transformed)
}

console.log(`built ${files.length} files to dist/`)
