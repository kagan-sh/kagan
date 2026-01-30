Here is a comprehensive set of rules and guidelines for an AI system to understand the Textual framework, covering architecture, widgets, layout, events, and best practices.

```markdown
# Textual Framework: System Rules & Guidelines

## 1. Core Architecture & App Lifecycle

* **App Class**: All applications must inherit from `textual.app.App`.
* **Entry Point**: Applications are started by calling the `.run()` method on an instance of the `App` class.
* **Composition**: Use the `compose()` method to define the initial UI. This method must yield `Widget` instances. Avoid adding widgets manually in `__init__` unless strictly necessary.
* **Mounting**: The `on_mount()` handler is called when the app or widget is added to the DOM. Use this for setup logic (e.g., loading data, starting timers) that requires the app to be running.
* **Inline Mode**: Apps can run in "inline mode" (rendering below the command prompt without clearing the screen) by passing `inline=True` to `.run()`.
* **Async Native**: Textual is built on `asyncio`. Event handlers (`on_key`, `on_mount`) can be `async` defs. Blocking operations inside handlers will freeze the UI; offload them to [Workers].

## 2. Widget Fundamentals

* **Base Class**: All UI components inherit from `textual.widget.Widget`.
* **Render vs. Compose**:
    * Use `compose()` to build "Compound Widgets" (widgets made of other widgets).
    * Use `render()` to draw content (strings or Rich renderables) directly onto the widget's surface.
    * Do not generally use both; if both exist, `render` acts as a background.
* **Static**: Use `textual.widgets.Static` for simple text or Rich content that needs to be updated via `.update()`.
* **IDs and Classes**:
    * Assign `id="my-id"` for unique identification (querying).
    * Assign `classes="class1 class2"` for grouping and styling.
* **DOM Interaction**:
    * Use `self.query_one("#id")` to find a single specific widget.
    * Use `self.query(".class")` or `self.query("Type")` to find multiple widgets.
    * Avoid storing references to widgets in `__init__`; query them when needed to ensure they are mounted.

## 3. Standard Library Widgets

* **Layouts**: Use `Vertical`, `Horizontal`, `Grid`, `Center`, `Middle` (from `textual.containers`) to arrange child widgets. Prefer context managers (e.g., `with Vertical():`) for nesting.
* **Input/Interaction**:
    * `Button`: Standard click interactions. Handle `Button.Pressed`.
    * `Input`: Single-line text entry. Handle `Input.Changed` or `Input.Submitted`. Supports validators.
    * `Switch` / `Checkbox` / `RadioButton`: Boolean toggles.
    * `Select` / `SelectionList`: Dropdowns and multi-select lists.
    * `TextArea`: Multi-line text/code editing with syntax highlighting (requires `textual[syntax]`).
* **Data Display**:
    * `DataTable`: Efficient scrolling grid for rows/columns. Supports sorting, cursors, and cell selection.
    * `Tree` / `DirectoryTree`: Hierarchical data display.
    * `RichLog`: efficient logging of text or Rich renderables (scrolling).
    * `MarkdownViewer`: Renders Markdown with a table of contents.
* **Tabs**: Use `TabbedContent` with `TabPane` for tabbed interfaces.

## 4. Textual CSS (TCSS)

* **Separation of Concerns**: Define styles in an external `.tcss` file referenced by the `CSS_PATH` class variable on the App.
* **Selectors**: Supports CSS-like selectors:
    * Type: `Button { ... }`
    * ID: `#submit { ... }`
    * Class: `.sidebar { ... }`
    * Nesting: `.sidebar Button { ... }` (descendant combinator).
    * Pseudo-classes: `:hover`, `:focus`, `:disabled`, `:dark`, `:light`.
* **Box Model**: Supports `margin`, `border`, `padding`, `width`, `height`.
    * `box-sizing: border-box` (default) includes padding/border in width/height.
* **Layout Modes**:
    * `dock: left/right/top/bottom`: Fixes a widget to an edge.
    * `layout: vertical`: Stacks children (default for Screen/Vertical).
    * `layout: horizontal`: Places children side-by-side.
    * `layout: grid`: Uses `grid-size`, `grid-columns`, `grid-rows` for complex 2D layouts.
* **Units**:
    * `fr`: Fractional units (e.g., `1fr`, `3fr`) for flexible space division.
    * `%`: Percentage of parent dimension.
    * `w`/`h`: Percentage of viewport width/height (e.g., `50w`).
    * Integer: Fixed character cells (e.g., `width: 20`).

## 5. Reactivity & State Management

* **Reactive Attributes**: Use `textual.reactive.reactive` to define state variables.
    * `count = reactive(0)`
* **Watchers**: Define `watch_<attribute_name>(self, old_val, new_val)` methods. Textual calls these automatically when the reactive value changes.
* **Compute Methods**: Define `compute_<attribute_name>` to create derived state that updates automatically.
* **Validators**: Define `validate_<attribute_name>` to sanitize input before assignment.
* **Automatic Refresh**: Changing a reactive attribute automatically repaints the widget (unless `layout=True` or `repaint=False` is adjusted).

## 6. Events & Message Passing

* **Handlers**: Define methods named `on_<event_name>` (e.g., `on_button_pressed`, `on_key`).
* **Decorator**: Prefer the `@on(EventClass, selector)` decorator for specific handling.
    * Example: `@on(Button.Pressed, "#submit")`
* **Bubbling**: Events bubble up the DOM from the focused/target widget to the App.
* **Stop Propagation**: Call `event.stop()` to prevent parent widgets from seeing an event.
* **Custom Messages**: Define subclasses of `textual.message.Message`. Use `self.post_message(MyMessage())` to send them. Parents handle them via `on_my_message`.

## 7. Input Handling

* **Bindings**: Define `BINDINGS` list in App or Widget classes for keyboard shortcuts.
    * Format: `("key", "action_name", "Description")`
    * Actions map to methods named `action_<action_name>`.
* **Focus**: Only the focused widget receives Key events. Use `widget.focus()` to set focus programmatically.
* **Mouse**: Handlers `on_click`, `on_mouse_move`, `on_mouse_down`, `on_mouse_up`.

## 8. Workers & Concurrency

* **Non-blocking**: Never use `time.sleep()` or heavy blocking calculations in the main thread.
* **@work Decorator**: Use `@work` to run methods as background tasks.
    * `@work(exclusive=True)` ensures only one instance of the worker runs (cancels previous).
    * `@work(thread=True)` runs the code in a thread (for blocking I/O).
* **Updating UI from Workers**:
    * In async workers: Update reactive attributes directly.
    * In threaded workers: Use `self.app.call_from_thread(callback)` to update UI safely.

## 9. Screens & Modes

* **Screen Stack**: The App maintains a stack of Screens.
* **Navigation**:
    * `self.push_screen(MyScreen())`: Overlay a new screen.
    * `self.pop_screen()`: Go back.
    * `self.switch_screen(MyScreen())`: Replace the current screen.
* **Modal**: Inherit from `textual.screen.ModalScreen` for popups/dialogs (supports transparency).
* **Modes**: Use `MODES` dict in App to define named screens (e.g., "dashboard", "settings") and switch between them.

## 10. Best Practices & Pitfalls

* **Always call super()**: When overriding `__init__`, `compose`, or lifecycle methods, ensure you call `super()` if required (always required for `__init__`).
* **Type Hinting**: Use type hints (e.g., `ComposeResult`, `RenderResult`) to leverage Textual's strong typing support.
* **CSS vs Code**: Keep layout and styling in CSS. Keep logic in Python. Do not hardcode styles in Python unless dynamic.
* **Compose vs Mount**: Create widgets in `compose`. Fetch data/configure widgets in `on_mount`.
* **Command Palette**: It is built-in (`Ctrl+P`). You can add custom commands via `get_system_commands`.
* **Debugging**: Use `textual run --dev myapp.py` and `textual console` to view logs and DOM structure live. Use `self.log()` instead of `print()`.

```
