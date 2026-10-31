// @ts-check
import { View, div } from "gpui-kit";
import { v_flex } from "gpui-base";
import { KgButton } from "gpui-component";
import { addComment, context, onContextChanged } from "kagan/host";
import { diff } from "kagan/threads";

export default class Plugin extends View {
  render() {
    return "";
  }
}

/** kagan's own footer counts, discards and sends the notes this view adds. */
export class Diff extends View {
  init(_props, cx) {
    this.path = "";
    this.lines = [];
    const read = (value) => {
      const props = value.props;
      if (!props || !props.threadId) return;
      this.path = props.path;
      Promise.resolve()
        .then(() => diff(props.threadId, props.scope, props.path))
        .then(
          (shown) => {
            this.lines = shown.hunks.flatMap((hunk) => [
              { text: `@@ -${hunk.oldStart} +${hunk.newStart} @@`, line: null },
              ...hunk.lines.map((line) => ({
                text: `${line.kind === "add" ? "+" : line.kind === "remove" ? "-" : " "}${line.text}`,
                line: line.new,
              })),
            ]);
            cx.notify();
          },
          (error) => {
            this.lines = [{ text: String(error), line: null }];
            cx.notify();
          },
        );
    };
    read(context());
    cx.spawn(async () => {
      for (;;) {
        read(await onContextChanged());
      }
    });
  }

  render() {
    const rows = this.lines.map((row, ix) =>
      row.line === null
        ? div().child(row.text)
        : new KgButton(`plain-diff-${ix}`)
            .label(row.text)
            .ghost()
            .on_click(() =>
              addComment({ path: this.path, start: row.line, end: row.line, text: "Please explain this line." }),
            ),
    );
    return v_flex().font_family("monospace").child(div().child(this.path)).children(rows);
  }
}
