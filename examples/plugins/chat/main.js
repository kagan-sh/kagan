// @ts-check
import { View, div } from "gpui-kit";
import { h_flex, v_flex } from "gpui-base";
import { Markdown } from "gpui-component";
import { context, onContextChanged } from "kagan/host";
import { draft, items, setDraft, waitItem } from "kagan/chat";
import { fail, next, reply } from "kagan/jobs";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind !== "command" || job.command !== "improve") {
          fail(job.id, `Unknown job ${job.kind}`);
          continue;
        }
        Promise.resolve()
          .then(() => draft(job.threadId))
          .then((text) => setDraft(job.threadId, `Improve this, keep it short:\n\n${text}`))
          .then(() => reply(job.id, null), (error) => fail(job.id, String(error)));
      }
    });
  }

  render() {
    return "";
  }
}

/** The transcript body. The prompt box, Send, Cancel and approvals stay kagan's. */
export class Transcript extends View {
  init(_props, cx) {
    this.cx = cx;
    this.thread = null;
    this.rows = [];
    this.read(context());
    cx.spawn(async () => {
      for (;;) this.read(await onContextChanged());
    });
  }

  /** @param {import("gpui-shell").Props | undefined} value */
  read(value) {
    const id = value && value.threadId;
    if (!id || id === this.thread) return;
    this.thread = id;
    this.rows = [];
    this.follow(id).catch((error) => {
      this.rows = [{ kind: "notice", text: String(error) }];
      this.cx.notify();
    });
  }

  /** Read the log forward, then wait for the next item, as `kagan/chat` documents. */
  async follow(id) {
    let after = 0;
    while (this.thread === id) {
      const page = await items(id, { afterSeq: after, order: "asc" });
      for (const item of page) after = Math.max(after, item.seq);
      if (page.length > 0) this.push(page);
      const item = await waitItem(id, { afterSeq: after, timeoutMs: 30_000 });
      if (item) {
        after = Math.max(after, item.seq);
        this.push([item]);
      }
    }
  }

  push(rows) {
    this.rows = [...this.rows, ...rows].slice(-200);
    this.cx.notify();
  }

  /** @param {import("gpui-kit").Context} cx */
  render(cx) {
    const colors = cx.theme().colors;
    const rows = this.rows.map((item) => {
      if (item.kind === "message") return new Markdown(`chat-message-${item.seq}`, item.text);
      if (item.kind === "work") {
        const done = item.work.status === "running" ? "Running" : item.work.status === "error" ? "Failed" : "Done";
        return h_flex()
          .id(`chat-work-${item.work.id}`)
          .h(28)
          .gap_2()
          .items_center()
          .child(div().text_size(12).text_color(colors.muted_foreground).child(done))
          .child(div().font_family("monospace").child(item.work.title));
      }
      if (item.kind === "notice") {
        return div().text_size(12).text_color(colors.muted_foreground).child(item.text);
      }
      if (item.kind === "turn") return div().h(1).bg(colors.border);
      return div();
    });
    return v_flex().size_full().gap_2().children(rows);
  }
}
