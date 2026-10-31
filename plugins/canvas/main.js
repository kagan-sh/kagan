// @ts-check

import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { openPanel } from "kagan/host";
import { changes, dump } from "./model.js";
import { editReported, list, peek } from "./store.js";

export { CanvasPanel } from "./panel.js";

export default class Plugin extends View {
  /** @param {import("gpui-shell").Props | undefined} _props @param {import("gpui-kit").AsyncContext} cx */
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then(
          (value) => reply(job.id, value),
          (error) => fail(job.id, error instanceof Error ? error.message : String(error)),
        );
      }
    });
  }

  /** @param {import("gpui-kit").Context} cx */
  render(cx) {
    return "";
  }
}

/** @param {import("kagan/jobs").Job} job */
async function handle(job) {
  if (job.kind === "tool") return tool(job);
  if (job.kind === "cli") return cli(job);
  if (job.kind === "command") {
    if (job.command === "open") await openPanel("canvas");
    return null;
  }
  throw new Error(`Unknown job ${job.kind}: ${typeof job.name === "string" ? job.name : ""}`);
}

/** @param {import("kagan/jobs").Job} job */
async function tool(job) {
  const input = job.input !== null && typeof job.input === "object" ? /** @type {Record<string, unknown>} */ (job.input) : {};
  try {
    if (job.name === "canvas_read") return { text: await readCanvas(typeof input.name === "string" ? input.name : undefined) };
    if (job.name === "canvas_edit") {
      if (typeof input.name !== "string" || input.name.trim() === "") throw new Error("canvas_edit needs a name");
      if (!Array.isArray(input.ops)) throw new Error("canvas_edit needs ops");
      const reported = await editReported(input.name, input.ops);
      return { text: changes(reported.before, reported.result.doc, reported.result.added) };
    }
    throw new Error(`Unknown tool ${String(job.name)}`);
  } catch (error) {
    return { text: error instanceof Error ? error.message : String(error), isError: true };
  }
}

/** @param {import("kagan/jobs").Job} job */
async function cli(job) {
  const command = typeof job.command === "string" ? job.command : "";
  const argv = Array.isArray(job.argv) ? job.argv.map((item) => String(item)) : [];
  try {
    if (command === "list" && argv.length === 0) {
      const names = await list();
      return { exitCode: 0, stdout: `${names.length === 0 ? "No canvases yet." : names.join("\n")}\n`, stderr: "" };
    }
    if (command === "show" && argv.length > 0) {
      return { exitCode: 0, stdout: `${await readCanvas(argv.join(" "))}\n`, stderr: "" };
    }
    return { exitCode: 1, stdout: "", stderr: "Usage: kg canvas list\n       kg canvas show <name>\n" };
  } catch (error) {
    return { exitCode: 1, stdout: "", stderr: `${error instanceof Error ? error.message : String(error)}\n` };
  }
}

/** @param {string | undefined} name */
async function readCanvas(name) {
  if (name === undefined) {
    const names = await list();
    return names.length === 0 ? "No canvases yet." : names.join("\n");
  }
  const doc = await peek(name);
  if (doc === null) {
    const names = await list();
    const resolved = name.trim().toLowerCase().replace(/\s+/g, "-");
    throw new Error(`No canvas named "${resolved}". ${names.length === 0 ? "There are no canvases yet." : `Known canvases: ${names.join(", ")}.`}`);
  }
  return dump(doc);
}
