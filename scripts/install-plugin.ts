import { cp, mkdir, rename, rm, writeFile } from "node:fs/promises"
import { homedir } from "node:os"
import { join, resolve } from "node:path"

const repoRoot = resolve(import.meta.dir, "..")
const snapshotDir = join(homedir(), ".kagan", "plugin", "kagan-pinned")
const prodPackageDir = join(homedir(), ".kagan", "plugin", "prod")
const opencodeCacheDir = process.env.XDG_CACHE_HOME
  ? join(process.env.XDG_CACHE_HOME, "opencode")
  : join(homedir(), ".cache", "opencode")
const globalConfigDir = process.env.XDG_CONFIG_HOME
  ? join(process.env.XDG_CONFIG_HOME, "opencode")
  : join(homedir(), ".config", "opencode")
const globalConfigFiles = [join(globalConfigDir, "opencode.json"), join(globalConfigDir, "tui.json")]

const snapshotEntries = ["src", "package.json", "tsconfig.json", "bun.lock", "node_modules"]

function defaultConfigFor(file: string): Record<string, unknown> {
  return file.endsWith("tui.json")
    ? { $schema: "https://opencode.ai/tui.json" }
    : { $schema: "https://opencode.ai/config.json" }
}

// string-aware so a `, }` inside a quoted value is never touched
function stripTrailingCommas(text: string): string {
  let out = ""
  let inString = false
  for (let i = 0; i < text.length; i++) {
    const ch = text.charAt(i)
    if (inString) {
      out += ch
      if (ch === "\\") out += text[++i] ?? ""
      else if (ch === '"') inString = false
      continue
    }
    if (ch === '"') inString = true
    else if (ch === ",") {
      let j = i + 1
      while (j < text.length && /\s/.test(text.charAt(j))) j++
      if (text[j] === "}" || text[j] === "]") continue
    }
    out += ch
  }
  return out
}

function parseConfigJson(text: string, file: string): Record<string, unknown> {
  try {
    return JSON.parse(text) as Record<string, unknown>
  } catch {
    // hand-edited OpenCode configs often keep a trailing comma
    try {
      return JSON.parse(stripTrailingCommas(text)) as Record<string, unknown>
    } catch (error) {
      throw new Error(`Failed to parse ${file}: ${error instanceof Error ? error.message : String(error)}`)
    }
  }
}

async function readConfig(file: string): Promise<Record<string, unknown>> {
  if (!(await Bun.file(file).exists())) return defaultConfigFor(file)
  return parseConfigJson(await Bun.file(file).text(), file)
}

async function addGlobalPluginSpec(spec: string): Promise<void> {
  await mkdir(globalConfigDir, { recursive: true })
  for (const file of globalConfigFiles) {
    const config = await readConfig(file)
    const plugins = Array.isArray(config.plugin) ? (config.plugin as unknown[]) : []
    config.plugin = plugins.includes(spec) ? plugins : [...plugins, spec]
    await writeFile(file, `${JSON.stringify(config, null, 2)}\n`)
    console.log(`${file} → plugin includes "${spec}"`)
  }
}

function pluginSpec(entry: unknown): string | undefined {
  if (typeof entry === "string") return entry
  if (!Array.isArray(entry)) return
  return typeof entry[0] === "string" ? entry[0] : undefined
}

function isKaganSpec(spec: string): boolean {
  return (
    spec.startsWith(snapshotDir) ||
    spec.startsWith(`file:${prodPackageDir}/`) ||
    spec === "@kagan-sh/kagan" ||
    spec.startsWith("@kagan-sh/kagan@")
  )
}

async function removeGlobalKaganPluginSpecs(): Promise<void> {
  for (const file of globalConfigFiles) {
    if (!(await Bun.file(file).exists())) continue
    const config = await readConfig(file)
    const plugins = Array.isArray(config.plugin) ? (config.plugin as unknown[]) : []
    const kept = plugins.filter((entry) => {
      const spec = pluginSpec(entry)
      return spec === undefined || !isKaganSpec(spec)
    })
    if (kept.length === plugins.length) continue
    if (kept.length > 0) config.plugin = kept
    else delete config.plugin
    if (Object.keys(config).length <= 1) {
      await rm(file, { force: true })
      console.log(`${file} → removed (kagan was its only content)`)
    } else {
      await writeFile(file, `${JSON.stringify(config, null, 2)}\n`)
      console.log(`${file} → dropped kagan plugin entries`)
    }
  }
}

async function run(args: string[], options?: { cwd?: string; stdout?: "pipe" | "inherit" }): Promise<string> {
  const proc = Bun.spawn(args, {
    cwd: options?.cwd ?? repoRoot,
    stdout: options?.stdout ?? "pipe",
    stderr: "inherit",
  })
  const [stdout, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited])
  if (code !== 0) throw new Error(`${args.join(" ")} failed`)
  return stdout
}

function parsePackedFilename(stdout: string): string {
  const packed = JSON.parse(stdout) as Array<{ filename?: string }>
  const filename = packed[0]?.filename
  if (!filename) throw new Error("npm pack returned no package")
  return filename
}

async function gitDescription(): Promise<string> {
  const proc = Bun.spawn(["git", "-C", repoRoot, "describe", "--always", "--dirty"], {
    stdout: "pipe",
    stderr: "ignore",
  })
  const [out] = await Promise.all([new Response(proc.stdout).text(), proc.exited])
  return out.trim() || "unknown"
}

async function buildSnapshot(): Promise<void> {
  const staging = `${snapshotDir}.staging`
  await rm(staging, { recursive: true, force: true })
  await mkdir(staging, { recursive: true })
  for (const entry of snapshotEntries) {
    await cp(join(repoRoot, entry), join(staging, entry), { recursive: true })
  }
  await writeFile(join(staging, "PINNED_FROM"), `${await gitDescription()} ${new Date().toISOString()}\n`)
  // rename-aside avoids ENOTEMPTY when replacing the snapshot dir
  const previous = `${snapshotDir}.old`
  await rm(previous, { recursive: true, force: true })
  await rename(snapshotDir, previous).catch(() => undefined)
  await rename(staging, snapshotDir)
  await rm(previous, { recursive: true, force: true })

  for (const entry of ["src/server.ts", "src/tui.tsx"]) {
    await import(join(snapshotDir, entry))
  }
}

async function installDev(): Promise<void> {
  await buildSnapshot()
  await addGlobalPluginSpec(snapshotDir)
  console.log(`Pinned snapshot: ${snapshotDir}`)
  console.log("kagan loads in every folder. Re-run `bun run plugin:install` after edits, then restart opencode.")
}

async function packProductionPlugin(): Promise<string> {
  await rm(prodPackageDir, { recursive: true, force: true })
  await mkdir(prodPackageDir, { recursive: true })
  const filename = parsePackedFilename(await run(["npm", "pack", "--json", "--pack-destination", prodPackageDir]))
  return join(prodPackageDir, filename)
}

async function installProd(): Promise<void> {
  const tgz = await packProductionPlugin()
  await removeGlobalKaganPluginSpecs()
  await rm(join(opencodeCacheDir, "packages", `file:${tgz}`), { recursive: true, force: true })
  await run(["opencode", "plugin", `file:${tgz}`, "--global", "--force"], { stdout: "inherit" })
  console.log(`Packed production plugin: ${tgz}`)
  console.log("kagan loads from the packed package. Restart opencode to apply.")
}

async function reset(): Promise<void> {
  await removeGlobalKaganPluginSpecs()
  console.log("Removed kagan plugin entries from the global config. Restart opencode to apply.")
}

const mode = Bun.argv[2]
if (mode === "dev") await installDev()
else if (mode === "prod") await installProd()
else if (mode === "reset") await reset()
else {
  console.error("Usage: bun scripts/install-plugin.ts <dev|prod|reset>")
  process.exit(1)
}
