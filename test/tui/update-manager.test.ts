import { afterEach, describe, expect, test } from "bun:test"
import { lstat, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { cleanupPreparedUpdate, prepareUpdate } from "../../src/tui/updates/manager"
import { wrapperTarget } from "../../src/tui/updates/paths"

const roots: string[] = []

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })))
})

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "kagan-update-"))
  roots.push(root)
  const scope = join(root, "opencode", "packages", "@kagan-sh")
  const current = join(scope, "kagan@latest")
  const target = wrapperTarget(current)
  await writePackage(target, "0.1.0")
  return { root, scope, current, target }
}

async function writePackage(target: string, version: string) {
  await mkdir(target, { recursive: true })
  await writeFile(join(target, "package.json"), JSON.stringify({ name: "@kagan-sh/kagan", version }))
}

async function writeWrapper(wrapper: string, version: string) {
  await writePackage(wrapperTarget(wrapper), version)
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

function markerPaths(scope: string, preparedVersion: string) {
  const current = join(scope, "kagan@latest")
  return {
    current,
    prepared: join(scope, `kagan@${preparedVersion}`),
    backup: join(scope, "kagan@latest.kagan-backup"),
    marker: join(scope, "kagan@latest.kagan-update.json"),
  }
}

async function writeMarker(scope: string, preparedVersion: string) {
  const paths = markerPaths(scope, preparedVersion)
  await writeFile(paths.marker, JSON.stringify({ version: preparedVersion }))
  return paths
}

const nodeFs = { lstat, readFile, rename, rm, writeFile }

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
    const preparedTarget = wrapperTarget(join(layout.scope, "kagan@0.2.0"))
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

  test("failed second rename restores the current wrapper", async () => {
    const layout = await fixture()
    const preparedTarget = wrapperTarget(join(layout.scope, "kagan@0.2.0"))
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
  })

  test("a failed restore surfaces both the promotion and restore errors", async () => {
    const layout = await fixture()
    const preparedTarget = wrapperTarget(join(layout.scope, "kagan@0.2.0"))
    const mock = api(async () => {
      await writePackage(preparedTarget, "0.2.0")
      return true
    })
    let renames = 0
    const fs = {
      ...nodeFs,
      rename: async (from: Parameters<typeof rename>[0], to: Parameters<typeof rename>[1]) => {
        renames++
        if (renames === 2) throw new Error("promotion failed")
        if (renames === 3) throw new Error("restore failed")
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
    let thrown: unknown
    try {
      await mock.disposers[0]!()
    } catch (error) {
      thrown = error
    }
    expect(thrown).toBeInstanceOf(AggregateError)
    expect((thrown as AggregateError).errors.map((entry) => (entry as Error).message)).toEqual([
      "promotion failed",
      "restore failed",
    ])
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

  test("removes the leftover prepared directory when clearing a matched marker's backup", async () => {
    const layout = await fixture()
    await writePackage(layout.target, "0.2.0")
    const paths = markerPaths(layout.scope, "0.2.0")
    await writeMarker(layout.scope, "0.2.0")
    await writeWrapper(paths.backup, "0.1.0")
    await writeWrapper(paths.prepared, "0.2.0")

    await cleanupPreparedUpdate(meta(layout.target), "0.2.0")

    expect(await lstat(paths.backup).catch(() => undefined)).toBeUndefined()
    expect(await lstat(paths.prepared).catch(() => undefined)).toBeUndefined()
    expect(await lstat(paths.marker).catch(() => undefined)).toBeUndefined()
    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.2.0")
  })

  test("removes an orphan backup when no marker matches", async () => {
    const layout = await fixture()
    const backup = join(layout.scope, "kagan@latest.kagan-backup")
    await writeWrapper(backup, "0.1.0")

    await cleanupPreparedUpdate(meta(layout.target), "0.1.0")

    expect(await lstat(backup).catch(() => undefined)).toBeUndefined()
    expect(JSON.parse(await readFile(join(layout.target, "package.json"), "utf8")).version).toBe("0.1.0")
  })

  test("removes a stale marker and its prepared directory", async () => {
    const layout = await fixture()
    const paths = await writeMarker(layout.scope, "0.2.0")
    await writeWrapper(paths.prepared, "0.2.0")

    await cleanupPreparedUpdate(meta(layout.target), "0.1.0")

    expect(await lstat(paths.marker).catch(() => undefined)).toBeUndefined()
    expect(await lstat(paths.prepared).catch(() => undefined)).toBeUndefined()
  })

  test("prepareUpdate succeeds after each stale-state variant is cleaned", async () => {
    for (const setup of [
      async (layout: Awaited<ReturnType<typeof fixture>>) => {
        const backup = join(layout.scope, "kagan@latest.kagan-backup")
        await writeWrapper(backup, "0.1.0")
      },
      async (layout: Awaited<ReturnType<typeof fixture>>) => {
        const paths = await writeMarker(layout.scope, "0.2.0")
        await writeWrapper(paths.prepared, "0.2.0")
      },
      async (layout: Awaited<ReturnType<typeof fixture>>) => {
        const prepared = join(layout.scope, "kagan@0.2.0")
        await writeWrapper(prepared, "0.2.0")
        await writeMarker(layout.scope, "0.2.0")
      },
    ]) {
      const layout = await fixture()
      await setup(layout)
      await cleanupPreparedUpdate(meta(layout.target), "0.1.0")

      const preparedTarget = wrapperTarget(join(layout.scope, "kagan@0.2.0"))
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
    }
  })
})
