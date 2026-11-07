export interface View {
  threadId: string;
  thread: string;
  url: string;
  title: string;
  shown: boolean;
}
export interface Control {
  ref: number;
  role: string;
  name: string;
  value?: string;
  focused: boolean;
  disabled: boolean;
  checked?: boolean;
}
export interface Snapshot {
  url: string;
  title: string;
  controls: Control[];
  text: string;
  truncated: boolean;
}
/** Gives this thread the Browser tab and loads `url`. */
export function open(threadId: string, url: string): Promise<View>;
export function status(threadId: string): Promise<View>;
export function snapshot(threadId: string): Promise<Snapshot>;
export function goto(threadId: string, url: string): Promise<void>;
export function back(threadId: string): Promise<void>;
export function forward(threadId: string): Promise<void>;
export function reload(threadId: string): Promise<void>;
/** `ref` is a whole number from the latest snapshot. */
export function click(threadId: string, ref: number): Promise<void>;
/** Writes `text` into the control. There is no file argument. */
export function fill(threadId: string, ref: number, text: string): Promise<void>;
export function type(threadId: string, text: string): Promise<void>;
export function press(threadId: string, key: string): Promise<void>;
export function select(threadId: string, ref: number, option: string): Promise<void>;
export function scroll(threadId: string, dy: number): Promise<void>;
export function close(threadId: string): Promise<void>;
