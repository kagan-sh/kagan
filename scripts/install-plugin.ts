import { cp, mkdir, rename, rm, writeFile } from "node:fs/promises"
import { homedir } from "node:os"
import { join, resolve } from "node:path"

const repoRoot = resolve(import.meta.dir, "..")
const snapshotDir = join(homedir(), ".kagan", "plugin", "kagan-pinned")
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

async function readConfig(file: string): Promise<Record<string, unknown>> {
  return (await Bun.file(file).exists())
    ? ((await Bun.file(file).json()) as Record<string, unknown>)
    : defaultConfigFor(file)
}

async function prettify(files: string[]): Promise<void> {
  const prettier = Bun.spawn(["bun", "x", "prettier", "--write", ...files], {
    cwd: repoRoot,
    stdout: "ignore",
    stderr: "inherit",
  })
  await prettier.exited
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
  await prettify(globalConfigFiles)
}

async function removeGlobalPluginSpecs(prefix: string): Promise<void> {
  const touched: string[] = []
  for (const file of globalConfigFiles) {
    if (!(await Bun.file(file).exists())) continue
    const config = await readConfig(file)
    const plugins = Array.isArray(config.plugin) ? (config.plugin as unknown[]) : []
    const kept = plugins.filter((entry) => typeof entry !== "string" || !entry.startsWith(prefix))
    if (kept.length === plugins.length) continue
    if (kept.length > 0) config.plugin = kept
    else delete config.plugin
    if (Object.keys(config).length <= 1) {
      await rm(file, { force: true })
      console.log(`${file} → removed (kagan was its only content)`)
    } else {
      await writeFile(file, `${JSON.stringify(config, null, 2)}\n`)
      console.log(`${file} → dropped kagan plugin entries`)
      touched.push(file)
    }
  }
  if (touched.length > 0) await prettify(touched)
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

async function reset(): Promise<void> {
  await removeGlobalPluginSpecs(snapshotDir)
  console.log("Removed the local kagan pin from the global config. Restart opencode to apply.")
}

const mode = Bun.argv[2]
if (mode === "dev") await installDev()
else if (mode === "reset") await reset()
else {
  console.error("Usage: bun scripts/install-plugin.ts <dev|reset>")
  process.exit(1)
}
