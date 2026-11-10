# Types and check

`kagan.d.ts` lists every `kagan/*` export. `kg plugin check <path>` refuses a script the host throws on when the plugin loads or a panel opens: an import that list does not have, a `gpui-component` state constructor whose arguments miss the component schema, and `KgInput` or `KgTextarea` given the `gpui-base` state of the same name. Check does not run the plugin.
