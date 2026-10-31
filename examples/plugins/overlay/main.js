// @ts-check

import { div, View } from "gpui-kit";
import { context, onContextChanged, settings } from "kagan/host";

const SIZE = 96;

export default class Plugin extends View {
  render() {
    return "Overlay";
  }
}

// An overlay may only paint: boxes, text, images, svgs and paths with their styles and motion.
// No handlers, focus, popups or window calls. `context().overlay` is the size of the area it covers.
export class Bubble extends View {
  /** @param {import("gpui-shell").Props | undefined} _props @param {import("gpui-kit").AsyncContext} cx */
  init(_props, cx) {
    this.area = context().overlay ?? { width: 0, height: 0 };
    /** @type {import("gpui-kit").Task | null} */
    this.tick = null;
    this.x = 24;
    this.y = 24;
    this.dx = 3;
    this.dy = 2;
    cx.spawn(async () => {
      for (;;) {
        this.area = (await onContextChanged()).overlay ?? this.area;
        cx.notify();
      }
    });
    // The fast tick runs only while the bubble is shown; a slow one notices the setting.
    cx.timer.every(1000, (cx) => this.follow(cx));
    this.follow(cx);
  }

  /** @param {import("gpui-kit").AsyncContext} cx */
  follow(cx) {
    const shown = settings().shown !== false;
    if (shown === (this.tick !== null)) return;
    if (shown) {
      this.tick = cx.timer.every(40, (cx) => {
        this.step();
        cx.notify();
      });
    } else {
      this.tick?.cancel();
      this.tick = null;
      cx.notify();
    }
  }

  step() {
    const right = Math.max(0, this.area.width - SIZE);
    const bottom = Math.max(0, this.area.height - SIZE);
    if (this.x + this.dx < 0 || this.x + this.dx > right) this.dx = -this.dx;
    if (this.y + this.dy < 0 || this.y + this.dy > bottom) this.dy = -this.dy;
    this.x = Math.min(right, Math.max(0, this.x + this.dx));
    this.y = Math.min(bottom, Math.max(0, this.y + this.dy));
  }

  /** @param {import("gpui-kit").Context} cx */
  render(cx) {
    if (this.tick === null) return div();
    const colors = cx.theme().colors;
    const size = `${Math.round(this.area.width)} × ${Math.round(this.area.height)}`;
    return div()
      .size_full()
      .child(div().absolute().left(this.x).top(this.y).w(SIZE).h(SIZE).rounded(SIZE / 2).bg(colors.primary).opacity(0.3))
      .child(div().absolute().left(this.x).top(this.y + SIZE + 4).text_size(12).text_color(colors.muted_foreground).child(size));
  }
}
