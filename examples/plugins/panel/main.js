import { View } from "gpui-kit";

export default class Plugin extends View {
  render() {
    return "Panel";
  }
}

export class Panel extends View {
  render() {
    return "Hello";
  }
}
