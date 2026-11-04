export type Status = "pending" | "idle" | "starting" | "active" | "stopping" | "error";
export interface Thread {
  id: string; projectId: string; environmentId: string | null; title: string;
  status: Status; visibility: "visible" | "hidden";
  parentThreadId: string | null; lifecycleOwnerThreadId: string | null;
  sectionId: string | null; pinned: boolean; lastReadAt: number | null;
  latestAttentionAt: number; archivedAt: number | null; createdAt: number; updatedAt: number;
}
export interface Project { id: string; kind: "standard" | "personal"; name: string; path: string; gitRemoteUrl: string | null }
export interface Environment { id: string; projectId: string; path: string; status: "creating" | "provisioning" | "ready" | "error" | "destroyed"; provider: "project" | "worktree" | "personal"; branch: string | null; baseBranch: string | null; retireAt: number | null }
export interface Sidebar { projects: Project[]; sections: { id: string; name: string }[]; threads: Thread[] }
export type EnvironmentSpec = { kind: "project-default" } | { kind: "reuse"; environmentId: string } | { kind: "worktree"; baseBranch?: string; branch?: string } | { kind: "personal" };
export interface Spawn { projectId: string; program: string; title?: string; environment: EnvironmentSpec; visibility?: "visible" | "hidden"; parentThreadId?: string; lifecycleOwnerThreadId?: string; metadata?: unknown }
export type DiffScope = "uncommitted" | "branch" | "commit";
export interface DiffFile { path: string; oldPath: string | null; added: number; removed: number; binary: boolean; status: "added" | "modified" | "deleted" | "renamed" }
export interface Changes { files: DiffFile[]; added: number; removed: number; base: string | null; repository: boolean }
export interface DiffLine { kind: "context" | "add" | "remove"; old: number | null; new: number | null; text: string }
export interface Diff { path: string; revision: string; binary: boolean; truncated: boolean; tooLarge: boolean; hunks: { oldStart: number; newStart: number; lines: DiffLine[] }[]; file: string | null; added: number; removed: number }
export interface Event { threadId: string; seq: number; type: string; at: number; data?: unknown }
export function get(id: string): Promise<Thread>;
export function list(options?: { projectId?: string; archived?: boolean; limit?: number; before?: number }): Promise<Thread[]>;
export function spawn(options: Spawn): Promise<Thread>;
export function stop(id: string): Promise<void>;
/** Types `text` into the running program and presses Enter. At most 1 MiB. */
export function send(id: string, text: string): Promise<void>;
/** The program's kept scrollback as plain text, at most 256 KiB. */
export function output(id: string): Promise<{ output: string }>;
export function archive(id: string): Promise<void>;
export function answer(id: string, interactionId: string, optionId: string): Promise<void>;
export function unarchive(id: string): Promise<void>;
export function remove(id: string): Promise<void>;
export function setPinned(id: string, pinned: boolean): Promise<void>;
export function rename(id: string, title: string): Promise<Thread>;
export function markRead(id: string): Promise<void>;
export function metadata(id: string): Promise<unknown>;
export function setMetadata(id: string, value: unknown): Promise<void>;
export function events(id: string, options: { afterSeq?: number; limit?: number; order?: "asc" | "desc" }): Promise<Event[]>;
export function waitEvent(id: string, options: { type: string; afterSeq?: number; timeoutMs: number }): Promise<Event | null>;
export function wait(id: string, options: { timeoutMs: number }): Promise<"waiting" | "exited" | "failed" | "not-busy" | "timeout">;
export function ask(id: string, title: string, options: { id: string; label: string }[]): Promise<string>;
export function sidebar(): Sidebar;
/** Changed files of a thread in this window's thread list. */
export function changes(id: string, scope: DiffScope): Promise<Changes>;
/** One changed file's patch, and its new side when that fits. */
export function diff(id: string, scope: DiffScope, path: string): Promise<Diff>;
export function projects(): Promise<Project[]>;
export function project(id: string): Promise<Project>;
export function environment(id: string): Promise<Environment>;
export function restoreEnvironment(threadId: string): Promise<Environment>;
