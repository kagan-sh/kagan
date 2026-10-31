// @ts-check

import { migrate, query, transaction } from "kagan/database";
import { project } from "kagan/threads";
import { applyOps, emptyDoc, normalizeName, parseDoc, slug } from "./model.js";

const UNDO_DEPTH = 25;
/** @type {Promise<void> | null} */
let migrated = null;
let generation = 0;
/** @type {Array<(generation: number) => void>} */
const waiters = [];
/** @type {Promise<void>} */
let chain = Promise.resolve();
/** @type {Map<string, { doc: import("./model.js").Doc, history: import("./model.js").ElementRecord[][], future: import("./model.js").ElementRecord[][] }>} */
const cache = new Map();

/** @template T @param {() => Promise<T>} fn */
function serial(fn) {
  const run = chain.then(fn);
  chain = run.then(() => undefined, () => undefined);
  return run;
}

export function ready() {
  migrated ??= migrate([
    `CREATE TABLE IF NOT EXISTS diagrams (
      name TEXT PRIMARY KEY,
      doc TEXT NOT NULL,
      history TEXT NOT NULL,
      future TEXT NOT NULL,
      updated_at INTEGER NOT NULL
    )`,
  ]);
  return migrated;
}

function touch() {
  generation += 1;
  const pending = waiters.splice(0);
  for (const resolve of pending) resolve(generation);
}

/** @param {number} since */
export function nextChange(since) {
  if (since !== generation) return Promise.resolve(generation);
  return new Promise((resolve) => waiters.push(resolve));
}

/** @param {string} diagramName */
async function row(diagramName) {
  const rows = await query("SELECT doc, history, future FROM diagrams WHERE name = ?", [diagramName]);
  return rows[0];
}

/** @param {string} diagramName */
async function loadRow(diagramName) {
  const cached = cache.get(diagramName);
  if (cached) return cached;
  const found = await row(diagramName);
  if (found === undefined) return null;
  const doc = parseDoc(JSON.parse(String(found.doc)), diagramName);
  const history = JSON.parse(String(found.history));
  const future = JSON.parse(String(found.future));
  if (!Array.isArray(history) || !Array.isArray(future)) throw new Error(`canvas "${diagramName}" history is unreadable`);
  const loaded = { doc, history, future };
  cache.set(diagramName, loaded);
  return loaded;
}

/** @param {import("./model.js").Doc} doc @param {import("./model.js").ElementRecord[][]} history @param {import("./model.js").ElementRecord[][]} future */
async function save(doc, history, future) {
  await transaction([{
    sql: `INSERT INTO diagrams (name, doc, history, future, updated_at) VALUES (?, ?, ?, ?, ?)
      ON CONFLICT(name) DO UPDATE SET doc = excluded.doc, history = excluded.history, future = excluded.future, updated_at = excluded.updated_at`,
    params: [doc.name, JSON.stringify(doc), JSON.stringify(history.slice(-UNDO_DEPTH)), JSON.stringify(future.slice(-UNDO_DEPTH)), Date.now()],
  }]);
  cache.set(doc.name, { doc, history: history.slice(-UNDO_DEPTH), future: future.slice(-UNDO_DEPTH) });
  touch();
}

export function list() {
  return serial(async () => {
    await ready();
    const rows = await query("SELECT name FROM diagrams ORDER BY updated_at DESC", []);
    return rows.map((item) => String(item.name));
  });
}

/** @param {string} diagramName */
export function peek(diagramName) {
  const name = normalizeName(diagramName);
  return serial(async () => {
    await ready();
    const loaded = await loadRow(name);
    return loaded === null ? null : loaded.doc;
  });
}

/** Creates the canvas the first time the panel opens it. */
export function open(diagramName) {
  const name = normalizeName(diagramName);
  return serial(async () => {
    await ready();
    const loaded = await loadRow(name);
    if (loaded !== null) return loaded.doc;
    const doc = emptyDoc(name);
    await save(doc, [], []);
    return doc;
  });
}

/** @param {string} diagramName */
async function requireExisting(diagramName) {
  const name = normalizeName(diagramName);
  const loaded = await loadRow(name);
  if (loaded === null) {
    const rows = await query("SELECT name FROM diagrams ORDER BY updated_at DESC", []);
    const known = rows.map((item) => String(item.name));
    throw new Error(`No canvas named "${name}". ${known.length === 0 ? "There are no canvases yet." : `Known canvases: ${known.join(", ")}.`}`);
  }
  return loaded;
}

/** @param {string} diagramName @param {unknown[]} ops */
export function edit(diagramName, ops) {
  return editReported(diagramName, ops).then((reported) => reported.result);
}

/** @param {string} diagramName @param {unknown[]} ops */
export function editReported(diagramName, ops) {
  const name = normalizeName(diagramName);
  return serial(async () => {
    await ready();
    const loaded = await requireExisting(name);
    const result = applyOps(loaded.doc, ops);
    await save(result.doc, [...loaded.history, loaded.doc.elements], []);
    return { before: loaded.doc, result };
  });
}

/** @param {string} diagramName @param {"undo" | "redo"} direction */
export function step(diagramName, direction) {
  const name = normalizeName(diagramName);
  return serial(async () => {
    await ready();
    const loaded = await requireExisting(name);
    const history = [...loaded.history];
    const future = [...loaded.future];
    const from = direction === "undo" ? history : future;
    const to = direction === "undo" ? future : history;
    const target = from.pop();
    if (target === undefined) throw new Error(`Nothing to ${direction} on "${name}"`);
    to.push(loaded.doc.elements);
    const doc = { ...loaded.doc, rev: loaded.doc.rev + 1, elements: /** @type {import("./model.js").ElementRecord[]} */ (target) };
    await save(doc, history, future);
    return doc;
  });
}

/** @param {string} diagramName */
export function remove(diagramName) {
  const name = normalizeName(diagramName);
  return serial(async () => {
    await ready();
    await transaction([
      { sql: "DELETE FROM diagrams WHERE name = ?", params: [diagramName] },
      { sql: "DELETE FROM diagrams WHERE name = ?", params: [name] },
    ]);
    cache.delete(name);
    touch();
    const rows = await query("SELECT name FROM diagrams ORDER BY updated_at DESC", []);
    return rows.map((item) => String(item.name));
  });
}

/** @param {string | undefined} projectId */
export async function projectCanvasName(projectId) {
  if (projectId === undefined || projectId === "") return "canvas";
  try {
    const found = await project(projectId);
    return slug(found.name);
  } catch {
    return slug(projectId.replace(/^proj_/, ""));
  }
}
