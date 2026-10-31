// @ts-check
import { View, div } from "gpui-kit";
import { v_flex } from "gpui-base";
import { KgButton } from "gpui-component";
import { context, onContextChanged, openThread } from "kagan/host";
import { sidebar } from "kagan/threads";

export default class Plugin extends View {
  render() {
    return "";
  }
}

export class List extends View {
  init(_props, cx) {
    this.listing = sidebar();
    this.shown = context().threadId;
    cx.spawn(async () => {
      for (;;) {
        this.shown = (await onContextChanged()).threadId;
        this.listing = sidebar();
        cx.notify();
      }
    });
  }

  render() {
    const rows = this.listing.projects.flatMap((project) => [
      div().child(project.name),
      ...this.listing.threads
        .filter((thread) => thread.projectId === project.id)
        .map((thread) => {
          const unread = thread.lastReadAt === null || thread.latestAttentionAt > thread.lastReadAt;
          return new KgButton(`plain-list-${thread.id}`)
            .label(`${unread ? "• " : ""}${thread.title} (${thread.status})`)
            .ghost()
            .selected(thread.id === this.shown)
            .on_click(() => openThread(thread.id));
        }),
    ]);
    return v_flex().gap_1().children(rows);
  }
}
