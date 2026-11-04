// @ts-check
import { View, div } from 'gpui-kit';
import { KgButton } from 'gpui-component';
import { log } from 'kagan/host';
import { next, reply, fail } from 'kagan/jobs';

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then(value => reply(job.id, value), error => fail(job.id, String(error)));
      }
    });
  }
}

export class HelloPanel extends View {
  init() {
    this.count = 0;
  }

  render() {
    return div().child(
      new KgButton('count')
        .label(`Count ${this.count}`)
        .on_click((_event, cx) => {
          this.count++;
          cx.notify();
        }),
    );
  }
}

async function handle(job) {
  if (job.kind === 'command' && job.command === 'hello') {
    log('info', 'hello from the starter plugin');
    return null;
  }
  throw new Error(`Unknown job ${job.kind}: ${job.name ?? ''}`);
}
