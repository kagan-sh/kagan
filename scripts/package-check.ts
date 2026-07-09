import { mkdtemp, mkdir, readdir, rm, stat, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

const repoRoot = resolve(import.meta.dir, "..")

type PackedFile = { path: string }
type PackResult = { filename: string; files: PackedFile[] }

const rawJsxPattern = /<(?:box|text)\b/

async function run(args: string[], cwd = repoRoot) {
  const proc = Bun.spawn(args, { cwd, stdout: "pipe", stderr: "pipe" })
  const [stdout, stderr, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ])
  if (code !== 0) throw new Error(`${args.join(" ")} failed\n${stdout}${stderr}`)
  return stdout
}

async function expectedPackageFiles() {
  const files = new Set(["package.json", "README.md", "LICENSE", "tsconfig.json", "bun.lock"])
  for await (const file of new Bun.Glob("dist/**/*").scan({ cwd: repoRoot, onlyFiles: true })) {
    if ((await stat(join(repoRoot, file))).isFile()) files.add(file)
  }
  return [...files].sort()
}

function parsePackJson(stdout: string) {
  const parsed = JSON.parse(stdout) as PackResult[]
  const first = parsed[0]
  if (!first) throw new Error("npm pack returned no package")
  return first
}

function assertSameFiles(actual: string[], expected: string[]) {
  const missing = expected.filter((file) => !actual.includes(file))
  const extra = actual.filter((file) => !expected.includes(file))
  if (missing.length || extra.length) {
    throw new Error(
      `package file list mismatch\nmissing: ${missing.join(", ") || "none"}\nextra: ${extra.join(", ") || "none"}`,
    )
  }
}

function assertCompiledSolid(file: string) {
  const source = Bun.file(join(repoRoot, file))
  if (!source.size) throw new Error(`${file} is empty`)
  return source.text().then((code) => {
    if (rawJsxPattern.test(code)) throw new Error(`${file} still contains raw JSX`)
    if (!code.includes("createComponent")) throw new Error(`${file} missing compiled Solid output`)
  })
}

async function assertNoBundledHostDeps(pluginRoot: string) {
  const nested = join(pluginRoot, "node_modules")
  if (!(await Bun.file(nested).exists())) return
  const entries = await readdir(nested)
  for (const name of ["@opentui", "solid-js"]) {
    if (entries.includes(name)) {
      throw new Error(`packed install bundles host dependency under ${nested}/${name}`)
    }
  }
}

await run(["bun", "scripts/build.ts"])
await Promise.all([assertCompiledSolid("dist/tui.js"), assertCompiledSolid("dist/board.js")])

const expected = await expectedPackageFiles()
assertSameFiles(
  parsePackJson(await run(["npm", "pack", "--dry-run", "--json"]))
    .files.map((file) => file.path)
    .sort(),
  expected,
)

const dir = await mkdtemp(join(tmpdir(), "kagan-package-"))
try {
  const packed = parsePackJson(await run(["npm", "pack", "--json", "--pack-destination", dir]))
  const consumer = join(dir, "consumer")
  await mkdir(consumer)
  await writeFile(
    join(consumer, "package.json"),
    JSON.stringify(
      {
        type: "module",
        dependencies: {
          "@kagan-sh/kagan": `file:${join(dir, packed.filename)}`,
          "@opentui/core": "0.4.3",
          "@opentui/keymap": "0.4.3",
          "@opentui/solid": "0.4.3",
          "solid-js": "1.9.12",
        },
      },
      null,
      2,
    ),
  )
  await run(["npm", "install", "--ignore-scripts", "--omit=dev", "--no-audit", "--no-fund"], consumer)
  const pluginRoot = join(consumer, "node_modules", "@kagan-sh", "kagan")
  await assertNoBundledHostDeps(pluginRoot)
  await run(
    [
      "bun",
      "--conditions",
      "browser",
      "-e",
      `import { ensureRuntimePluginSupport } from "@opentui/solid/runtime-plugin-support/configure"
import { runtimeModules } from "@opentui/keymap/runtime-modules"
ensureRuntimePluginSupport({ additional: runtimeModules })
const server = await import("@kagan-sh/kagan/server")
const tui = await import("@kagan-sh/kagan/tui")
if (typeof server.default?.server !== "function") throw new Error("server export missing")
if (typeof tui.default?.tui !== "function") throw new Error("tui export missing")`,
    ],
    consumer,
  )
} finally {
  await rm(dir, { recursive: true, force: true })
}

console.log("package check passed")
