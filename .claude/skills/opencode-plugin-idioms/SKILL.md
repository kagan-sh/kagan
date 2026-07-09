---
name: opencode-plugin-idioms
description: Event hook payload shapes, session lifecycle timing, toast/TUI-route render boundaries, and child-session tagging conventions for OpenCode server (@opencode-ai/plugin) and TUI (@opencode-ai/plugin/tui) plugins, grounded in the vendored OpenCode source at references/opencode.
type: prompt
whenToUse: Load before modifying src/server.ts, src/tui.tsx, src/tui/board/board.tsx, src/tui/board/commands.tsx, src/tui/board/store.tsx, src/tui/updates.ts, src/tui/update-manager.ts, src/server/intake.ts, or src/server/validator/spawn.ts — or when debugging a missing toast, a session-creation race, a permission/gate ordering bug, or a plugin-spawned child session re-triggering its own handler.
---

Version check: repo pins `@opencode-ai/plugin@1.17.18`; vendored copy at `references/opencode/packages/plugin/package.json` should match — verify there, never from memory.

## 1. Event hook: what fires, exact payload, and ordering guarantees

`Hooks.event` receives `{ event: Event }` where `Event` is a tagged union — `references/opencode/packages/plugin/src/index.ts:224` and the generated union at `references/opencode/packages/sdk/js/src/gen/types.gen.ts:704-736`.

Session-lifecycle members (the ones this repo's `src/server.ts` `event` hook switches on):

| type                 | properties                                  | fires when                                                                                                                  |
| -------------------- | ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `session.created`    | `{ info: Session }`                         | `client.session.create()` resolves — distinct event, **not** folded into `session.updated`. Schema: `types.gen.ts:562-567`. |
| `session.updated`    | `{ info: Session }`                         | any subsequent `session.update()` (metadata patch, title change, etc). `types.gen.ts:569-574`.                              |
| `session.deleted`    | `{ info: Session }`                         | `client.session.delete()`. `types.gen.ts:576-581`.                                                                          |
| `session.idle`       | `{ sessionID: string }`                     | agent turn finishes and session goes idle. `types.gen.ts:475-480`.                                                          |
| `session.status`     | `{ sessionID, status: SessionStatus }`      | status transition (running/idle/etc). `types.gen.ts:467-473`.                                                               |
| `session.compacted`  | `{ sessionID }`                             | compaction completes. `types.gen.ts:482-487`.                                                                               |
| `permission.updated` | `Permission`                                | a permission request is created/changed. `types.gen.ts:439-442`.                                                            |
| `permission.replied` | `{ sessionID, permissionID, response }`     | human answers a permission prompt. `types.gen.ts:444-450`.                                                                  |
| `command.executed`   | `{ name, sessionID, arguments, messageID }` | slash command runs. `types.gen.ts:523-529`.                                                                                 |
| `file.edited`        | `{ file }`                                  | `types.gen.ts:489-493`.                                                                                                     |
| `todo.updated`       | `{ sessionID, todos }`                      | `types.gen.ts:515-521`.                                                                                                     |
| `tui.toast.show`     | `{ title?, message, variant, duration }`    | published via `client.tui.showToast(...)` — see §3. `types.gen.ts` (search `EventTuiToastShow`).                            |

Note the wire shape differs from the raw schema definition: `packages/schema/src/v1/session.ts:571-587` defines `Created`/`Updated`/`Deleted` with both `sessionID` and `info` fields, but the generated SDK type collapses this to `properties: { info: Session }` only (`types.gen.ts:562-581`) — read the session ID off `info.id`, not `properties.sessionID`. This repo's `src/server.ts:246,252-253` already does this correctly.

**Creation timing — no race.** `Session.createNext` builds the row in memory and calls `yield* events.publish(SessionV1.Event.Created, { sessionID, info })` synchronously within the same Effect (`references/opencode/packages/opencode/src/session/session.ts:537`), before returning to the caller. The durable-event pipeline runs all registered _projectors_ — including the one that `db.insert(SessionTable, ...)` (`references/opencode/packages/core/src/session/projector.ts:215-224`) — synchronously and awaited (`for (const projector of list) { yield* projector(committed) }`, `references/opencode/packages/core/src/event.ts:320-322`) **before** `notify()` delivers the event to any listener (`event.ts:389` calls `notify` after `commitDurableEvent` resolves; `notify` fans out to plugin listeners at `event.ts:406-416`). Net effect: by the time a plugin's `event` hook receives `session.created`, `client.session.get({ path: { id } })` is guaranteed to find the row. No polling or retry needed.

**Fan-out is fire-and-forget, not awaited, not ordered across plugins.** The host's event dispatch loop is:

```ts
// references/opencode/packages/opencode/src/plugin/index.ts:251-258
const unsubscribe =
  yield *
  events.listen((event) => {
    if (event.location?.directory !== ctx.directory) return Effect.void
    return Effect.sync(() => {
      for (const hook of hooks) {
        void hook["event"]?.({ event: { id: event.id, type: event.type, properties: event.data } as any })
      }
    })
  })
```

`void hook["event"]?.(...)` — every plugin's `event` handler for the same event fires concurrently and the host does not await any of them. Two plugins reacting to the same `session.updated` by patching `metadata` can race each other (last `session.update()` write wins). Contrast with `(input, output)`-style hooks below.

## 2. Reacting to session creation

`session.created` (§1) covers instant reaction to a new session. This repo's `src/server.ts` `event` hook
switches on `session.created`, `session.updated`, `session.idle`, and others (`src/server.ts:477-491`).

`handleSessionCreated` (`src/server.ts:317-324`) runs on `session.created`: skips helper/parented
sessions (`role` or `parentID`), then for board tasks with no `lastGatedStatus` yet seeds it from
`getStatus(metadata)` via `patchKagan`. That establishes the baseline column before
`handleSessionUpdated` sees status changes.

`handleSessionUpdated` (`src/server.ts:326-386`) reads `lastGatedStatus` from persisted
`metadata.kagan` (not an in-memory Map). On first update where `prev === undefined`, it seeds
`lastGatedStatus` for board tasks (covers sessions created before the field existed or when
`session.created` raced). When `newCol !== prev`, it enforces column-move gates and updates
`lastGatedStatus` on allowed moves.

The `undefined` sentinel on `lastGatedStatus` distinguishes "never gated" from an explicit column
value — `session.created` seeds it early so subsequent `session.updated` events can gate column
transitions correctly.

Full `Hooks` interface (all hook names a plugin may implement) — `references/opencode/packages/plugin/src/index.ts:222-335`:

- `dispose`, `event`, `config`
- `tool` (map of custom tools), `auth`, `provider`
- `chat.message`, `chat.params`, `chat.headers`
- `permission.ask`
- `command.execute.before`
- `tool.execute.before`, `tool.execute.after`, `tool.definition`
- `shell.env`
- `experimental.chat.messages.transform`, `experimental.chat.system.transform`, `experimental.provider.small_model`, `experimental.session.compacting`, `experimental.compaction.autocontinue`, `experimental.text.complete`

All of these except `event`/`config`/`dispose` follow `(input, output) => Promise<void>` and run through `Plugin.trigger`, which is **sequential, awaited, in plugin-registration order**, each plugin mutating the same shared `output` object:

```ts
// references/opencode/packages/opencode/src/plugin/index.ts:280-293
for (const hook of s.hooks) {
  const fn = hook[name] as any
  if (!fn) continue
  yield * Effect.promise(async () => fn(input, output))
}
```

So a later-registered plugin can silently overwrite an earlier plugin's `output.status` (`permission.ask`, if wired) or `output.args` (`tool.execute.before`). This repo's own `tool.execute.before` handler reads `output` and throws only when it must block a command; it doesn't assume it's the only plugin present, which is correct defensive practice given this ordering.

**`permission.ask` is declared in the `Hooks` type but has zero trigger call sites** anywhere in `packages/opencode/src` in this snapshot (`grep -rn '"permission.ask"\|trigger("permission' packages/opencode/src` returns nothing outside the type definition and this repo's own handler). Don't assume it's guaranteed to fire on every permission prompt without testing against the actual runtime version in use — treat it as declared-but-unverified-wired for 1.17.x and confirm empirically if a permission gate silently doesn't trigger.

## 3. Toast from server-side plugin code

**`PluginInput` has no `ui` field at all.** Confirmed by its full type — `references/opencode/packages/plugin/src/index.ts:56-66` (`client`, `project`, `directory`, `worktree`, `experimental_workspace`, `serverUrl`, `$`). `api.ui.toast` only exists on `TuiPluginApi` (`references/opencode/packages/plugin/src/tui.ts:599-609`), which is exclusive to the TUI-side plugin (`tui: TuiPlugin`), not the server-side plugin (`server: Plugin`). A server-side `event` hook **cannot** call `api.ui.toast` — that method doesn't exist in that scope.

**The idiomatic server-side path is `client.tui.showToast({...})`.** It's a real, generated SDK method on both `@opencode-ai/sdk` (`references/opencode/packages/sdk/js/src/gen/sdk.gen.ts:1115-1127`, `POST /tui/show-toast`) and `@opencode-ai/sdk/v2` (`references/opencode/packages/sdk/js/src/v2/gen/sdk.gen.ts:4906-4933`). It publishes the server event `tui.toast.show` (schema: `references/opencode/packages/schema/src/tui-event.ts:40-50`), which any attached TUI process picks up:

```ts
// references/opencode/packages/tui/src/app.tsx:976-984
event.on("tui.toast.show", (evt, { workspace }) => {
  if (workspace !== project.workspace.current()) return
  toast.show({
    title: evt.properties.title,
    message: evt.properties.message,
    variant: evt.properties.variant,
    duration: evt.properties.duration,
  })
})
```

The TUI-side `api.ui.toast` does **not** go through this event at all — it's a direct in-process store mutation: `references/opencode/packages/tui/src/plugin/adapters.tsx:257-264` calls `input.toast.show(...)` synchronously. The server-published `tui.toast.show` event and the client-side `api.ui.toast` call are two independent paths that both terminate in the same `toast.show()` store setter, but only one crosses the process boundary.

**Gotcha — toast is invisible while a plugin's custom full-screen route is active.** `<Toast/>` is not mounted in the shared `App()` composition at all. It's hardcoded only inside the two built-in routes:

- `references/opencode/packages/tui/src/routes/home.tsx:88`
- `references/opencode/packages/tui/src/routes/session/index.tsx:1322`

The shared `App()` render tree switches on route type and renders a plugin's custom route via a separate `plugin()` memo with no toast wrapper:

```ts
// references/opencode/packages/tui/src/app.tsx:1096-1109
<Show when={ready()}>
  <box flexGrow={1} minHeight={0} flexDirection="column">
    <Switch>
      <Match when={route.data.type === "home"}><Home /></Match>
      <Match when={route.data.type === "session"}>...</Match>
    </Switch>
    {plugin()}   // <- kagan board renders here when ROUTE is navigated to; no <Toast/> nearby
  </box>
  ...
</Show>
```

When `route.data.type === "plugin"` (this repo's `api.route.navigate(ROUTE)` in `src/tui.tsx:29`), neither `<Switch>` `<Match>` fires, so `<Home/>`/`<Session/>` (and their embedded `<Toast/>`) don't mount. **A toast call made while the kagan board is the active route is not dropped by a race or clobbered visually — there is simply no `<Toast/>` component mounted to render it.** It is silently swallowed until the user navigates back to `home` or a `session` route. Use the board's own notice overlay for board-route feedback.

Automatic update status deliberately uses a separate dual surface: `api.ui.toast` once when the
current route is `home` or `session`, and persistent footer state on the Kagan route. It never enters
`store.notify`; operational task warnings retain the board Notice queue.

## 4. Spawning child/helper sessions without recursive triggering

`SessionInfo` has first-class `parentID?: SessionID` and free-form `metadata?: Record<string, unknown>` — `references/opencode/packages/schema/src/v1/session.ts:550,559`. There is **no built-in "internal/helper session" flag** anywhere in the schema or in any bundled OpenCode plugin (`references/opencode/packages/opencode/src/plugin/*` only contains auth-provider plugins; none spawn sessions). The tagging convention is project-invented, not an OpenCode-documented idiom — state that plainly rather than citing it as upstream guidance.

This repo already implements the correct pattern in `src/server/intake.ts:9-19` and `src/server/validator/spawn.ts:51-61`:

- Set `parentID: parentSessionID` on `session.create()` for the structural link.
- Set `metadata.kagan.role` to a discriminator (`"intake"` / `"validator"`) plus a back-pointer (`intakeParent` / `validatorParent`) so a downstream `event` handler can distinguish a helper session from a real task session by reading `session.metadata.kagan.role` before doing anything session-lifecycle-driven.
- `src/server.ts`'s `event` handler skips helper and parented sessions in `handleSessionUpdated` via `if (infoView.role || info.parentID) return` (`src/server.ts:328`) before running column-move gates — any new lifecycle handler on `session.updated` must apply the same guard first to avoid an intake/validator session recursively spawning its own intake/validator.

Setting `parentID` is not purely structural bookkeeping — it changes real framework behavior worth relying on for helper/child sessions:

- Skips auto-share: `references/opencode/packages/opencode/src/share/session.ts:41` — `if (result.parentID) return result`.
- Skips auto-title generation from the first user message: `references/opencode/packages/opencode/src/session/prompt.ts:199` — `if (input.session.parentID) return`.
- Gets a distinct default title prefix when no explicit title is passed: `"Child session - " + ISO timestamp` vs `"New session - " + ISO timestamp` (`references/opencode/packages/opencode/src/session/session.ts:48-49,523`).
- Excluded from the legacy CLI's top-level `--continue` session listing (`references/opencode/packages/opencode/src/cli/cmd/run.ts:492`).

None of this stops the child's own `session.created`/`session.updated` events from re-entering the same plugin `event` hook — there is no dispatch-level recursion filtering. `parentID` buys framework side-benefits only; the recursion guard is still entirely on the handler, via `metadata?.kagan?.role`/`*Parent` lookups (`src/server.ts`) and via skipping any `parentID`-bearing session in WIP/board counts (`src/server/data.ts`: `if (session.parentID) continue`).

## 5. Permission-config merging convention

`Hooks.config` receives the live, mutable `Config` object and plugins are invoked sequentially, each free to mutate it in place before the next plugin runs:

```ts
// references/opencode/packages/opencode/src/plugin/index.ts:241-249
for (const hook of hooks) {
  yield* Effect.tryPromise({ try: () => Promise.resolve((hook as any).config?.(cfg)), ... })
}
```

Because `cfg` is shared and mutated (not replaced per-plugin), the idiomatic merge is additive-preserve-existing, exactly as `src/server.ts:94-103` (`mergeBashPermission`) does: read `config.permission`, spread the existing `bash` rule map, then overlay this plugin's own rules on top — never assign a fresh object that would blow away another plugin's earlier permission entries.

## Gotchas summary (source-cited)

- `session.created` is a real, distinct event — don't infer creation from `session.updated`'s absence. `packages/schema/src/v1/session.ts:572-579`, `packages/sdk/js/src/gen/types.gen.ts:562-567`.
- No race on `client.session.get()` immediately after `session.created` — DB projector commits before listener fan-out. `packages/core/src/event.ts:320-322,389`; `packages/core/src/session/projector.ts:215-224`.
- Plugin `event` hooks run concurrently and unawaited relative to each other (`void hook["event"]?.(...)`) — no cross-plugin ordering guarantee, possible metadata-patch races. `packages/opencode/src/plugin/index.ts:254-256`.
- `(input, output)` hooks (`permission.ask`, `tool.execute.before`, etc.) run sequentially and awaited, sharing one mutable `output` — later plugins can overwrite earlier plugins' decisions. `packages/opencode/src/plugin/index.ts:280-293`.
- Server-side `PluginInput` has no `ui`/toast surface; use `client.tui.showToast(...)`, not `api.ui.toast`. `packages/plugin/src/index.ts:56-66` vs `packages/plugin/src/tui.ts:599-609`; SDK method at `packages/sdk/js/src/gen/sdk.gen.ts:1115-1127`.
- `<Toast/>` is not mounted while a plugin's custom full-screen route (`route.data.type === "plugin"`) is active — toasts fired during that time render nothing, not a visual clobber. `packages/tui/src/app.tsx:1096-1109` vs `1322` (session route) / `home.tsx:88`.
- No built-in "internal session" flag — `metadata.kagan.role` + `parentID`/`*Parent` back-pointer is this repo's own convention (`src/server/intake.ts`, `src/server/validator/spawn.ts`), not an OpenCode-documented idiom.
- `Config` passed to `Hooks.config` is shared/mutated across plugins in registration order — merge additively, never reassign wholesale. `packages/opencode/src/plugin/index.ts:241-249`.
- `permission.ask` is declared in `Hooks` but has no confirmed trigger call site in `packages/opencode/src` in this snapshot — verify empirically before relying on it firing. `packages/plugin/src/index.ts:261`.
- `parentID` on session create skips auto-share and auto-title generation and changes the default title prefix — real framework behavior, not just bookkeeping. `packages/opencode/src/share/session.ts:41`; `packages/opencode/src/session/prompt.ts:199`; `packages/opencode/src/session/session.ts:48-49,523`.
