/** A part of kagan's window the person picked. */
export type Picked = {
  role: string | null;
  label: string | null;
  value: string | null;
  path: string[];
  bounds: { x: number; y: number; width: number; height: number };
  owner: string | null;
};

/** Puts the front kagan window in pick mode. Resolves to what the person clicks, or null when they press Escape. */
export function start(): Promise<Picked | null>;
