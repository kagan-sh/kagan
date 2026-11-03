# Borrowed rules

Gates for behavior taken from claude, codex, or alacritty. Decisions 49, 50, and 52 say why. This table names the file, when to read the row again, and the test that goes red if the workaround is removed first. No loader and no runtime. Change the row in the same change as the code.

| Quirk | File | Re-check | Test |
|---|---|---|---|
| A title is busy only when its first character is U+2800–U+28FF or U+25D0–U+25D3. Those ranges are kagan's rule. | `crates/core/src/thread/terminal.rs` (`busy`) | when the installed claude or codex version changes | `a_spinner_that_is_not_first_is_not_busy` and the capture tests beside it, in `crates/core/src/thread/terminal/tests.rs` |
| `claude` starts with `--mcp-config`, `--plugin-dir`, and `--append-system-prompt-file`. `codex` gets that server as `-c mcp_servers.kagan.*`. `cursor-agent` starts with `--plugin-dir`. | `crates/core/src/thread/agent.rs` | when the installed claude, codex, or cursor-agent version changes | `a_claude_thread_gets_kagan_flags_env_and_skills`, `a_codex_thread_gets_the_server_and_note_as_overrides`, and `a_cursor_agent_thread_receives_the_threads_skill` in `crates/core/src/tests.rs` |
| Codex reads a skill linked at `<cwd>/.agents/skills`. | `crates/core/src/thread/agent.rs` (`link_skill`) | when the installed codex version changes | `a_codex_thread_gets_the_server_and_note_as_overrides` in `crates/core/src/tests.rs` |
| Known program names. `claude` and `codex` are always listed. | `crates/domain/src/program.rs` (`KNOWN_PROGRAMS`) | when the installed claude or codex version changes | `the_program_list_finds_known_agents_and_defaults_to_the_first_installed` in `crates/core/src/tests.rs` |
| A plain title becomes waiting only after one second. Codex writes a plain title between spinner frames. | `crates/core/src/thread/terminal.rs` (`QUIET`) | when the installed claude or codex version changes | `a_plain_title_waits_only_after_one_second` in `crates/core/src/thread/terminal/tests.rs` |
| `ESC c` clears the title inside alacritty without a `ResetTitle` event, so the thread keeps the title it already stored. | `crates/terminal/src/screen.rs` (cleared only on `ResetTitle`) | when the installed claude or codex version changes, and when `alacritty_terminal` changes | `a_full_reset_keeps_the_stored_title` in `crates/terminal/src/screen/tests.rs` |
