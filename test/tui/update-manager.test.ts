import { afterEach, describe, expect, test } from "bun:test"
import { link, lstat, mkdir, mkdtemp, readFile, realpath, rename, rm, symlink, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { cleanupPreparedUpdate, prepareUpdate } from "../../src/tui/update-manager"

const roots: string[] = []

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })))
})

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "kagan-update-"))
  roots.push(root)
  const scope = join(root, "opencode", "packages", "@kagan-sh")
  const current = join(scope, "kagan@latest")
  const target = join(current, "node_modules", "@kagan-sh", "kagan")
  await writePackage(target, "0.1.0")
  return { root, scope, current, target }
}

async function writePackage(target: string, version: string) {
  await mkdir(target, { recursive: true })
  await writeFile(join(target, "package.json"), JSON.stringify({ name: "@kagan-sh/kagan", version }))
}

function meta(target: string, overrides: Partial<TuiPluginMeta> = {}): TuiPluginMeta {
  return {
    id: "kagan",
    source: "npm",
    spec: "@kagan-sh/kagan",
    target,
    state: "same",
    first_time: 0,
    last_time: 0,
    time_changed: 0,
    load_count: 1,
    fingerprint: "test",
    ...overrides,
  }
}

function api(add: () => Promise<boolean>) {
  const disposers: Array<() => void | Promise<void>> = []
  return {
    value: {
      plugins: { add },
      lifecycle: {
        signal: new AbortController().signal,
        onDispose: (dispose: () => void | Promise<void>) => {
          disposers.push(dispose)
          return () => {}
        },
      },
    } as unknown as TuiPluginApi,
    disposers,
  }
}

const nodeFs = { lstat, readFile, realpath, rename, rm, writeFile }

describe("prepareUpdate", () => {
  test("rejects a non-release status before calling the host installer", async () => {
    const layout = await fixture()
    let called = false
    const mock = api(async () => {
      called = true
      return true
    })
    expect(
      await prepareUpdate({
        api: mock.value,
        meta: meta(layout.target),
        currentVersion: "0.1.0",
        status: { kind: "ready", version: "latest" },
      }),
    ).toBe(false)
    expect(called).toBe(false)
  })

  test("failed download or import leaves the current wrapper untouched", async () => {
    const layout = await fixture()
    const mock = api(async () => false)
    expect(
      await prepareUpdate({
        api: mock.value,
        meta: meta(layout.target),
        currentVersion: "0.1.0",
        status: { kind: "ready", version: "0.2.0" },
      }),
    ).toBe(false)
    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.1.0")
    expect(mock.disposers).toHaveLength(0)
  })

  test("prepares exact latest without touching current before disposal", async () => {
    const layout = await fixture()
    const preparedTarget = join(layout.scope, "kagan@0.2.0", "node_modules", "@kagan-sh", "kagan")
    const mock = api(async () => {
      await writePackage(preparedTarget, "0.2.0")
      return true
    })

    expect(
      await prepareUpdate({
        api: mock.value,
        meta: meta(layout.target),
        currentVersion: "0.1.0",
        status: { kind: "ready", version: "0.2.0" },
      }),
    ).toBe(true)
    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.1.0")
    expect(await lstat(join(layout.scope, "kagan@latest.kagan-update.json"))).toBeTruthy()
    expect(mock.disposers).toHaveLength(1)
  })

  test("rejects a symlinked cache wrapper", async () => {
    const layout = await fixture()
    const realPrepared = join(layout.root, "prepared")
    await writePackage(join(realPrepared, "node_modules", "@kagan-sh", "kagan"), "0.2.0")
    await symlink(realPrepared, join(layout.scope, "kagan@0.2.0"))
    const mock = api(async () => true)

    expect(
      await prepareUpdate({
        api: mock.value,
        meta: meta(layout.target),
        currentVersion: "0.1.0",
        status: { kind: "ready", version: "0.2.0" },
      }),
    ).toBe(false)
    expect(mock.disposers).toHaveLength(0)
  })

  test("rejects a hardlinked marker without modifying its source", async () => {
    const layout = await fixture()
    const preparedTarget = join(layout.scope, "kagan@0.2.0", "node_modules", "@kagan-sh", "kagan")
    const victim = join(layout.root, "victim.json")
    await writeFile(victim, "do not replace")
    await link(victim, join(layout.scope, "kagan@latest.kagan-update.json"))
    const mock = api(async () => {
      await writePackage(preparedTarget, "0.2.0")
      return true
    })

    expect(
      await prepareUpdate({
        api: mock.value,
        meta: meta(layout.target),
        currentVersion: "0.1.0",
        status: { kind: "ready", version: "0.2.0" },
      }),
    ).toBe(false)
    expect(await readFile(victim, "utf8")).toBe("do not replace")
    expect(mock.disposers).toHaveLength(0)
  })

  test("revalidates wrappers at disposal before touching current", async () => {
    const layout = await fixture()
    const prepared = join(layout.scope, "kagan@0.2.0")
    const preparedTarget = join(prepared, "node_modules", "@kagan-sh", "kagan")
    const mock = api(async () => {
      await writePackage(preparedTarget, "0.2.0")
      return true
    })
    await prepareUpdate({
      api: mock.value,
      meta: meta(layout.target),
      currentVersion: "0.1.0",
      status: { kind: "ready", version: "0.2.0" },
    })

    await rm(prepared, { recursive: true })
    const replacement = join(layout.root, "replacement")
    await writePackage(join(replacement, "node_modules", "@kagan-sh", "kagan"), "0.2.0")
    await symlink(replacement, prepared)

    await expect(mock.disposers[0]!()).rejects.toThrow("Unsafe Kagan cache path")
    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.1.0")
  })

  test("failed second rename restores the current wrapper", async () => {
    const layout = await fixture()
    const preparedTarget = join(layout.scope, "kagan@0.2.0", "node_modules", "@kagan-sh", "kagan")
    const mock = api(async () => {
      await writePackage(preparedTarget, "0.2.0")
      return true
    })
    let renames = 0
    const fs = {
      ...nodeFs,
      rename: async (from: Parameters<typeof rename>[0], to: Parameters<typeof rename>[1]) => {
        renames++
        if (renames === 2) throw new Error("second rename failed")
        await rename(from, to)
      },
    }

    expect(
      await prepareUpdate({
        api: mock.value,
        meta: meta(layout.target),
        currentVersion: "0.1.0",
        status: { kind: "ready", version: "0.2.0" },
        fs,
      }),
    ).toBe(true)
    await expect(mock.disposers[0]!()).rejects.toThrow("second rename failed")
    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.1.0")
    expect(renames).toBe(3)
  })

  test("successful next startup removes the marker and backup", async () => {
    const layout = await fixture()
    const preparedTarget = join(layout.scope, "kagan@0.2.0", "node_modules", "@kagan-sh", "kagan")
    const mock = api(async () => {
      await writePackage(preparedTarget, "0.2.0")
      return true
    })
    await prepareUpdate({
      api: mock.value,
      meta: meta(layout.target),
      currentVersion: "0.1.0",
      status: { kind: "ready", version: "0.2.0" },
    })
    await mock.disposers[0]!()
    await cleanupPreparedUpdate(meta(layout.target), "0.2.0")

    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.2.0")
    expect(await lstat(join(layout.scope, "kagan@latest.kagan-backup")).catch(() => undefined)).toBeUndefined()
    expect(await lstat(join(layout.scope, "kagan@latest.kagan-update.json")).catch(() => undefined)).toBeUndefined()
  })
})

describe("cleanupPreparedUpdate", () => {
  test("does nothing for exact pins", async () => {
    const layout = await fixture()
    const marker = join(layout.scope, "kagan@latest.kagan-update.json")
    await writeFile(marker, "{}")
    await cleanupPreparedUpdate(meta(layout.target, { spec: "@kagan-sh/kagan@0.1.0" }), "0.1.0")
    expect(await lstat(marker)).toBeTruthy()
  })

  test("keeps a marker whose paths do not match the validated layout", async () => {
    const layout = await fixture()
    const marker = join(layout.scope, "kagan@latest.kagan-update.json")
    await writeFile(marker, JSON.stringify({ version: "0.1.0", current: "/tmp/other", prepared: "", backup: "" }))
    await cleanupPreparedUpdate(meta(layout.target), "0.1.0")
    expect(await lstat(marker)).toBeTruthy()
  })

  test("rejects a hardlinked marker before cleanup", async () => {
    const layout = await fixture()
    const victim = join(layout.root, "victim.json")
    await writeFile(victim, "{}")
    await link(victim, join(layout.scope, "kagan@latest.kagan-update.json"))
    await expect(cleanupPreparedUpdate(meta(layout.target), "0.1.0")).rejects.toThrow("Unsafe Kagan update marker")
    expect(await readFile(victim, "utf8")).toBe("{}")
  })
})
