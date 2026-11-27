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
      this.rows = [{ key: "error", type: "notice", text: String(error), data: { severity: "error", text: String(error) } }];
      this.cx.notify();
    });
  }

  /** Read the log forward, then wait for the next event, as `kagan/chat` documents. */
  async follow(id) {
    let after = 0;
    while (this.thread === id) {
      const [page, asked] = await Promise.all([
        items(id, { afterSeq: after }),
        approvals(id),
      ]);
      if (this.thread !== id) return;
      for (const event of page) after = Math.max(after, event.seq);
      this.push(page, asked);
      const event = await waitItem(id, { afterSeq: after, timeoutMs: 30_000 });
      if (this.thread !== id) return;
      if (event) {
        after = Math.max(after, event.seq);
        this.push([event]);
      }
    }
  }

  /** Events are deltas: a message's chunks share `messageId`, and one tool call is several `tool`
   * events with one `id`, so fold those. `tool.started`/`tool.ended` share that id but are rows of
   * their own. */
  push(page, asked) {
    for (const event of page) {
      const data = event.data || {};
      const key =
        event.type === "tool" && data.id
          ? data.id
          : data.messageId || (event.type === "plan" ? "plan" : `event-${event.seq}`);
      const row = this.rows.find((row) => row.key === key);
      if (!row) this.rows.push({ key, type: event.type, text: data.text || "", data });
      else if (event.type === "message" || event.type === "thought") row.text += data.text || "";
      else row.data = { ...row.data, ...data, title: data.title || row.data.title };
    }
    this.rows = this.rows.slice(-200);
    if (asked) this.waiting = asked;
    this.cx.notify();
  }

  /** @param {import("gpui-kit").Context} cx */
  render(cx) {
    const colors = cx.theme().colors;
    const rows = this.rows.map((row) => {
      if (row.type === "message") return new Markdown(`chat-${row.key}`, row.text);
      if (row.type === "thought") {
        return div().text_size(12).text_color(colors.muted_foreground).child(row.text);
      }
      if (row.type === "plan") {
        return div()
          .text_size(12)
          .child((row.data.entries || []).map((entry) => entry.text).join(" · "));
      }
      if (row.type === "tool") {
        const call = row.data;
        const done = call.status === "completed" ? "Done" : call.status === "failed" ? "Failed" : "Running";
        return h_flex()
          .id(`chat-work-${call.id}`)
          .h(28)
          .gap_2()
          .items_center()
          .child(div().text_size(12).text_color(colors.muted_foreground).child(done))
          .child(div().font_family("monospace").child(call.title || call.name || call.id));
      }
      if (row.type === "notice") {
        return div().text_size(12).text_color(colors.muted_foreground).child(row.data.text);
      }
      if (row.type === "turn.ended") return div().h(1).bg(colors.border);
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
