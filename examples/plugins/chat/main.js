// @ts-check
import { View, div } from "gpui-kit";
import { h_flex, v_flex } from "gpui-base";
import { Markdown } from "gpui-component";
import { context, onContextChanged } from "kagan/host";
import { approvals, draft, items, setDraft, waitItem } from "kagan/chat";
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

/** The transcript body. The prompt box, Send, Cancel and the approvals stay kagan's. */
export class Transcript extends View {
  init(_props, cx) {
    this.cx = cx;
    this.thread = null;
    this.rows = [];
    this.waiting = [];
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
    this.waiting = [];
    this.follow(id).catch((error) => {
      this.rows = [{ kind: "notice", severity: "error", text: String(error) }];
      this.cx.notify();
    });
  }

  /** Read the log forward, then wait for the next item, as `kagan/chat` documents. */
  async follow(id) {
    let after = 0;
    while (this.thread === id) {
      const [page, asked] = await Promise.all([
        items(id, { afterSeq: after, order: "asc" }),
        approvals(id),
      ]);
      if (this.thread !== id) return;
      for (const item of page) after = Math.max(after, item.seq);
      this.push(page, asked);
      const item = await waitItem(id, { afterSeq: after, timeoutMs: 30_000 });
      if (this.thread !== id) return;
      if (item) {
        after = Math.max(after, item.seq);
        this.push([item]);
      }
    }
  }

  /** Items are deltas: a tool call's start and end fold into one row by `work.id`. */
  push(page, asked) {
    for (const item of page) {
      const row =
        item.kind === "work"
          ? this.rows.find((row) => row.kind === "work" && row.work.id === item.work.id)
          : undefined;
      if (row) row.work = { ...row.work, ...item.work, title: item.work.title || row.work.title };
      else this.rows.push(item);
    }
    this.rows = this.rows.slice(-200);
    if (asked) this.waiting = asked;
    this.cx.notify();
  }

  /** @param {import("gpui-kit").Context} cx */
  render(cx) {
    const colors = cx.theme().colors;
    const rows = this.rows.map((item) => {
      if (item.kind === "message") return new Markdown(`chat-message-${item.seq}`, item.text);
      if (item.kind === "thought") {
        return div().text_size(12).text_color(colors.muted_foreground).child(item.text);
      }
      if (item.kind === "plan") {
        return div()
          .text_size(12)
          .child(item.plan.steps.map((step) => step.step).join(" · "));
      }
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
    // The host draws the approval buttons; the rows only say that something waits.
    if (this.waiting.length > 0) {
      rows.push(
        div().id("chat-waiting").text_size(12).text_color(colors.muted_foreground).child("Waiting on you"),
      );
    }
    return v_flex().size_full().gap_2().children(rows);
  }
}
