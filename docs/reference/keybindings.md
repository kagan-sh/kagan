---
title: Keyboard Shortcuts
description: Every keybinding available in the TUI
icon: material/keyboard
---

# Keyboard Shortcuts

Mirrors the in-app help (press ++question++).

## Essential

| Key          | Action             |
| ------------ | ------------------ |
| ++n++        | New task           |
| ++enter++    | Open / confirm     |
| ++a++        | Start agent (AUTO) |
| ++s++        | Stop agent         |
| ++question++ | Help               |
| ++period++   | Actions palette    |
| ++comma++    | Settings           |
| ++q++        | Quit               |

## Global

| Key                     | Action           |
| ----------------------- | ---------------- |
| ++question++ / ++f1++   | Help             |
| ++period++ / ++ctrl+p++ | Actions palette  |
| ++ctrl+o++              | Project selector |
| ++ctrl+r++              | Repo selector    |
| ++f12++                 | Debug log        |
| ++q++                   | Quit             |

## Board (Kanban)

### Navigation

| Key               | Action                                   |
| ----------------- | ---------------------------------------- |
| ++h++ / ++left++  | Focus left column                        |
| ++l++ / ++right++ | Focus right column                       |
| ++j++ / ++down++  | Focus next card                          |
| ++k++ / ++up++    | Focus previous card                      |
| ++tab++           | Next column                              |
| ++shift+tab++     | Previous column                          |
| ++esc++           | Clear focus, close search, or close peek |

### Tasks

| Key         | Action                             |
| ----------- | ---------------------------------- |
| ++n++       | New task                           |
| ++shift+n++ | New AUTO task                      |
| ++enter++   | Open session (PAIR) or Task Output |
| ++slash++   | Search tasks                       |
| ++v++       | View details                       |
| ++e++       | Edit task                          |
| ++x++       | Delete task                        |
| ++y++       | Duplicate task                     |
| ++c++       | Copy task ID                       |
| ++space++   | Peek overlay                       |
| ++f++       | Expand description                 |
| ++f5++      | Full editor                        |

### Workflow and agents

| Key         | Action               |
| ----------- | -------------------- |
| ++shift+h++ | Move task left       |
| ++shift+l++ | Move task right      |
| ++a++       | Start agent (AUTO)   |
| ++s++       | Stop agent (AUTO)    |
| ++shift+d++ | View diff (REVIEW)   |
| ++r++       | Task Output (REVIEW) |
| ++m++       | Merge (REVIEW)       |
| ++p++       | Plan mode            |
| ++b++       | Set task branch      |
| ++shift+b++ | Set default branch   |
| ++comma++   | Settings             |
| ++ctrl+c++  | Quit                 |

## Planner

### Screen

| Key         | Action             |
| ----------- | ------------------ |
| ++esc++     | Back to board      |
| ++ctrl+c++  | Stop current run   |
| ++f2++      | Enhance prompt     |
| ++b++       | Set task branch    |
| ++shift+b++ | Set default branch |

### Input

| Key                          | Action             |
| ---------------------------- | ------------------ |
| ++enter++                    | Send message       |
| ++shift+enter++ / ++ctrl+j++ | New line           |
| `/help`                      | Show commands      |
| `/clear`                     | Clear conversation |

### Slash complete

| Key               | Action         |
| ----------------- | -------------- |
| ++up++ / ++down++ | Navigate list  |
| ++enter++         | Select command |
| ++esc++           | Dismiss list   |

### Plan approval

| Key                            | Action         |
| ------------------------------ | -------------- |
| ++up++/++down++ or ++j++/++k++ | Move selection |
| ++enter++                      | Preview task   |
| ++a++                          | Approve        |
| ++e++                          | Edit           |
| ++d++ / ++esc++                | Dismiss        |

## Welcome and onboarding

### Welcome screen

| Key            | Action                 |
| -------------- | ---------------------- |
| ++enter++      | Open selected project  |
| ++n++          | New project            |
| ++o++          | Open folder            |
| ++s++          | Settings               |
| ++1++ to ++9++ | Open project by number |
| ++esc++        | Quit                   |

### Onboarding

| Key     | Action |
| ------- | ------ |
| ++esc++ | Quit   |

## Repo picker

| Key                            | Action         |
| ------------------------------ | -------------- |
| ++up++/++down++ or ++j++/++k++ | Navigate repos |
| ++enter++                      | Select repo    |
| ++n++                          | Add repo       |
| ++esc++                        | Cancel         |

## Modals

### Help

| Key             | Action |
| --------------- | ------ |
| ++esc++ / ++q++ | Close  |

### Confirm

| Key       | Action  |
| --------- | ------- |
| ++enter++ | Confirm |
| ++y++     | Yes     |
| ++n++     | No      |
| ++esc++   | Cancel  |

### Task details

| Key                | Action             |
| ------------------ | ------------------ |
| ++e++              | Toggle edit        |
| ++d++              | Delete             |
| ++f++              | Expand description |
| ++f5++             | Full editor        |
| ++f2++ / ++alt+s++ | Save (edit mode)   |
| ++y++              | Copy               |
| ++esc++            | Close/Cancel       |

### Task editor

| Key                | Action         |
| ------------------ | -------------- |
| ++f2++ / ++alt+s++ | Finish editing |
| ++esc++            | Cancel         |

### Description editor

| Key                | Action |
| ------------------ | ------ |
| ++f2++ / ++alt+s++ | Save   |
| ++esc++            | Cancel |

### Settings

| Key                | Action |
| ------------------ | ------ |
| ++f2++ / ++alt+s++ | Save   |
| ++esc++            | Cancel |

### Duplicate task

| Key       | Action |
| --------- | ------ |
| ++enter++ | Create |
| ++esc++   | Cancel |

### Diff

| Key       | Action  |
| --------- | ------- |
| ++enter++ | Approve |
| ++r++     | Reject  |
| ++y++     | Copy    |
| ++esc++   | Close   |

### Task Output

| Key       | Action       |
| --------- | ------------ |
| ++enter++ | Approve      |
| ++r++     | Reject       |
| ++g++     | Run review   |
| ++y++     | Copy         |
| ++esc++   | Close/Cancel |

### Rejection input

| Key       | Action              |
| --------- | ------------------- |
| ++enter++ | Back to In Progress |
| ++esc++   | Backlog             |

### Agent and review chat input

| Key                          | Action       |
| ---------------------------- | ------------ |
| ++enter++                    | Send message |
| ++shift+enter++ / ++ctrl+j++ | New line     |

### Debug log

| Key     | Action     |
| ------- | ---------- |
| ++c++   | Clear logs |
| ++s++   | Save logs  |
| ++esc++ | Close      |

### Tmux gateway

| Key       | Action           |
| --------- | ---------------- |
| ++enter++ | Continue         |
| ++esc++   | Cancel           |
| ++s++     | Don't show again |

### Base branch

| Key       | Action |
| --------- | ------ |
| ++enter++ | Submit |
| ++esc++   | Cancel |

### Permission prompt

| Key             | Action       |
| --------------- | ------------ |
| ++enter++       | Allow once   |
| ++a++           | Allow always |
| ++esc++ / ++n++ | Deny         |

### No dedicated hotkeys

| Modal         | Notes                        |
| ------------- | ---------------------------- |
| Merge Dialog  | Use buttons and checkboxes   |
| New Project   | Use inputs and buttons       |
| Folder Picker | Use input, tree, and buttons |
