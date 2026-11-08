use super::{check_plugin, plugin_dir};

fn source(body: &str) -> String {
    if body.contains("export default") {
        body.to_owned()
    } else {
        format!("{body}\nexport default class Plugin {{}}\n")
    }
}

fn checked(body: &str) -> String {
    let dir = plugin_dir();
    std::fs::write(dir.path().join("main.js"), source(body)).unwrap();
    check_plugin(dir.path()).unwrap_err().to_string()
}

fn accepted(body: &str) {
    let dir = plugin_dir();
    std::fs::write(dir.path().join("main.js"), source(body)).unwrap();
    check_plugin(dir.path()).unwrap();
}

#[test]
fn check_refuses_a_host_import_the_module_does_not_export() {
    let error = checked("import { onSettingsChanged } from \"kagan/host\";");
    assert!(
        error.contains("imports onSettingsChanged from \"kagan/host\""),
        "{error}"
    );
    assert!(error.contains("does not export it"), "{error}");

    let error = checked("import { onSettingsChanged as watch } from \"kagan/host\";");
    assert!(error.contains("onSettingsChanged"), "{error}");

    let error = checked(
        "import * as host from \"kagan/host\";\nexport default class Plugin { init() { host.onSettingsChanged(); } }",
    );
    assert!(error.contains("onSettingsChanged"), "{error}");
    assert!(error.contains("kagan/host"), "{error}");
}

#[test]
fn check_accepts_the_host_exports_and_ignores_a_mention_in_text() {
    accepted("import { log, settings, context, onContextChanged } from \"kagan/host\";");
    accepted(
        "// import { onSettingsChanged } from \"kagan/host\"\nconst text = \"import { onSettingsChanged } from \\\"kagan/host\\\"\";",
    );
}

#[test]
fn check_refuses_a_component_state_built_with_the_base_options_object() {
    let textarea = checked(
        "import { TextareaState } from \"gpui-component\";\nexport default class Plugin { init() { this.note = TextareaState.new({ rows: 4, placeholder: \"Note\" }); } }",
    );
    assert!(textarea.contains("TextareaState.new"), "{textarea}");
    assert!(textarea.contains("not an object"), "{textarea}");
    assert!(textarea.contains("one optional string"), "{textarea}");

    let input = checked(
        "import { InputState } from \"gpui-component\";\nexport default class Plugin { init() { this.query = InputState.new({ placeholder: \"Find\" }); } }",
    );
    assert!(input.contains("InputState.new"), "{input}");
    assert!(input.contains("two optional strings"), "{input}");

    let extra = checked(
        "import { TextareaState } from \"gpui-component\";\nTextareaState.new(\"a\", \"b\");",
    );
    assert!(extra.contains("got 2 arguments; it takes 1"), "{extra}");
}

#[test]
fn check_accepts_the_component_state_constructors() {
    accepted(
        "import { InputState, TextareaState, Textarea } from \"gpui-component\";\nexport default class Plugin { init() { const note = TextareaState.new(\"hello\"); this.query = InputState.new(\"Find\", \"\"); this.field = new Textarea(note); } }",
    );
    accepted("import { TextareaState } from \"gpui-component\";\nTextareaState.new();");
}

#[test]
fn check_refuses_a_base_state_passed_to_the_component() {
    let error = checked(
        "import { Textarea } from \"gpui-component\";\nimport { TextareaState } from \"gpui-base\";\nexport default class Plugin { init() { const state = TextareaState.new({ placeholder: \"n\", value: \"a\", rows: 3 }); this.field = new Textarea(state); } }",
    );
    assert!(
        error.contains("expects the TextareaState from \"gpui-component\""),
        "{error}"
    );
    assert!(error.contains("this one is gpui-base's"), "{error}");
    assert!(!error.contains("not an object"), "{error}");
}

#[test]
fn check_names_the_script_that_imports_a_missing_export() {
    let dir = plugin_dir();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    std::fs::write(
        dir.path().join("ui/panel.js"),
        "import { onSettingsChanged } from \"kagan/host\";\n",
    )
    .unwrap();
    let error = check_plugin(dir.path()).unwrap_err().to_string();
    assert!(error.contains("ui/panel.js"), "{error}");
    assert!(error.contains("onSettingsChanged"), "{error}");
}
