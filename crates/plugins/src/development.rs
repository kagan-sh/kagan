use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{Read as _, Seek as _};
use std::path::{Path, PathBuf};

use kg_domain::{OpError, OpResult, PluginId};
use serde_json::Value;
use tree_sitter::Node;

use crate::{Result, discover};

fn name_too_long(error: std::io::Error) -> OpError {
    if error.kind() == std::io::ErrorKind::InvalidFilename {
        OpError::Invalid("the plugin name is too long".into())
    } else {
        error.into()
    }
}

pub(crate) fn new(
    name: &str,
    dir: Option<PathBuf>,
    components: &gpui_shell::FrozenComponentRegistry,
) -> Result<OpResult> {
    if name.is_empty()
        || !name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
    {
        return Err(OpError::Invalid(
            "plugin name must be lowercase and hyphenated".into(),
        ));
    }
    let root = dir.unwrap_or_else(|| PathBuf::from(name));
    if let Some(parent) = root.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(name_too_long)?;
    }
    std::fs::create_dir(&root).map_err(name_too_long)?;
    let manifest = serde_json::json!({ "id": format!("dev.kagan.{name}"), "name": name, "version": "0.1.0", "kagan": format!(">={}", env!("CARGO_PKG_VERSION")), "entry": "main.js", "instructions": "A starter plugin: a hello tool under the threads. A panel may also be a center page or a right tab. Bottom is refused. A hello command writes to its log; a person binds its key in Settings, Keyboard.", "powers": {}, "commands": [{ "id": "hello", "title": "Say hello" }], "ui": { "panels": [{ "id": "hello", "title": "Hello", "view": "HelloPanel", "placement": "left" }] }, "tools": [], "skills": [] });
    std::fs::write(
        root.join("kagan.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    std::fs::write(
        root.join("main.js"),
        "// @ts-check\nimport { View, div } from 'gpui-kit';\nimport { KgButton } from 'gpui-component';\nimport { log } from 'kagan/host';\nimport { next, reply, fail } from 'kagan/jobs';\n\nexport default class Plugin extends View {\n  init(_props, cx) {\n    cx.spawn(async () => {\n      for (;;) {\n        const job = await next();\n        handle(job).then(value => reply(job.id, value), error => fail(job.id, String(error)));\n      }\n    });\n  }\n}\n\nexport class HelloPanel extends View {\n  init() {\n    this.count = 0;\n  }\n\n  render() {\n    return div().child(\n      new KgButton('count')\n        .label(`Count ${this.count}`)\n        .on_click((_event, cx) => {\n          this.count++;\n          cx.notify();\n        }),\n    );\n  }\n}\n\nasync function handle(job) {\n  if (job.kind === 'command' && job.command === 'hello') {\n    log('info', 'hello from the starter plugin');\n    return null;\n  }\n  throw new Error(`Unknown job ${job.kind}: ${job.name ?? ''}`);\n}\n",
    )?;
    std::fs::write(
        root.join("jsconfig.json"),
        "{\n  \"compilerOptions\": { \"checkJs\": true, \"target\": \"ES2022\", \"module\": \"ESNext\", \"moduleResolution\": \"bundler\", \"noEmit\": true },\n  \"include\": [\"**/*.js\", \"*.d.ts\"]\n}\n",
    )?;
    let mut files = types(&root, components)?;
    files.extend([
        root.join("kagan.json"),
        root.join("main.js"),
        root.join("jsconfig.json"),
    ]);
    Ok(OpResult::Files(files))
}

/// Writes the declarations a plugin type-checks against. They come from the app's whole catalog,
/// so a component kagan adds is declared too.
pub(crate) fn types(
    root: &Path,
    components: &gpui_shell::FrozenComponentRegistry,
) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Err(OpError::Invalid(format!(
            "{} is not a folder",
            root.display()
        )));
    }
    let mut files = gpui_shell::write_type_declarations_with_components(root, components)?;
    let jobs = crate::jobs::Jobs::new();
    let modules = crate::area::modules(
        &PluginId("dev.kagan.types".into()),
        crate::area::Values {
            settings: std::rc::Rc::default(),
            secrets: crate::secret::Store::new(None, root.to_path_buf()),
        },
        &std::rc::Rc::default(),
        &jobs,
        None,
        root,
    );
    let mut declarations = String::new();
    for (_, module) in modules {
        let body = module
            .declared()
            .ok_or_else(|| OpError::Failed(format!("{} has no declarations", module.name())))?;
        write!(
            declarations,
            "declare module {:?} {{\n{body}\n}}\n",
            module.name()
        )
        .map_err(|error| OpError::Failed(error.to_string()))?;
    }
    let path = root.join("kagan.d.ts");
    std::fs::write(&path, declarations)?;
    files.push(path);
    Ok(files)
}

/// Checks a plugin's declarations and returns the problems that do not stop it from loading.
pub(crate) fn check(root: &Path, data_dir: &Path) -> Result<Vec<String>> {
    let discovered = discover(root, data_dir)?;
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(root.join("kagan.json"))?)?;
    let source = std::fs::read_to_string(root.join(&discovered.manifest.entry))?;
    let exports = exports(&source).map_err(|error| match error {
        OpError::Invalid(message) => {
            OpError::Invalid(format!("{}: {message}", discovered.manifest.entry))
        }
        other => other,
    })?;
    if !exports.contains("default") {
        return Err(OpError::Invalid(
            "entry must default-export a View class".into(),
        ));
    }
    let mut declared = Vec::new();
    contributions(&manifest["ui"], &mut declared);
    for name in declared {
        if !exports.contains(&name) {
            return Err(OpError::Invalid(format!(
                "declared view {name} is not exported by {}",
                discovered.manifest.entry
            )));
        }
    }
    plain_buttons(root)
}

/// A plain `Button` is not one `kg ui` can find or click. `KgButton` takes the same calls and can.
fn plain_buttons(root: &Path) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    for script in plugin_scripts(root)? {
        let source = std::fs::read_to_string(&script)?;
        if constructs_button(&source) {
            // Plugin paths use `/` on every platform, as in the manifest.
            let shown = script
                .strip_prefix(root)
                .unwrap_or(&script)
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            warnings.push(format!(
                "{shown} uses Button, which kg ui cannot find or click; use KgButton"
            ));
        }
    }
    Ok(warnings)
}

/// Every `.js` file under the plugin folder, sorted. Hidden folders and `node_modules` are skipped.
fn plugin_scripts(root: &Path) -> Result<Vec<PathBuf>> {
    let mut scripts = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                if !name.starts_with('.') && name != "node_modules" {
                    folders.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "js") {
                scripts.push(path);
            }
        }
    }
    scripts.sort();
    Ok(scripts)
}

/// Whether a script constructs `Button` with `new`. The syntax tree decides, so spacing, comments
/// and strings do not matter. A script that does not parse is left to the load to report.
fn constructs_button(source: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let mut nodes = vec![tree.root_node()];
    while let Some(node) = nodes.pop() {
        if node.kind() == "new_expression"
            && node
                .child_by_field_name("constructor")
                .is_some_and(|constructor| {
                    constructor.kind() == "identifier"
                        && &source[constructor.byte_range()] == "Button"
                })
        {
            return true;
        }
        let mut cursor = node.walk();
        nodes.extend(node.children(&mut cursor));
    }
    false
}

// Whether any script calls `.store(true)` on a chain that starts at `new WebView(...)`.
pub(crate) fn calls_webview_store(root: &Path) -> bool {
    plugin_scripts(root).is_ok_and(|scripts| {
        scripts
            .iter()
            .any(|path| std::fs::read_to_string(path).is_ok_and(|source| calls_store_true(&source)))
    })
}

// True when the chain starts at `new WebView(...)`, optionally followed by more calls such as `.url(...)`.
fn is_webview_chain(node: tree_sitter::Node, source: &str) -> bool {
    let mut node = node;
    loop {
        match node.kind() {
            "call_expression" => {
                let Some(function) = node.child_by_field_name("function") else {
                    return false;
                };
                if function.kind() != "member_expression" {
                    return false;
                }
                let Some(object) = function.child_by_field_name("object") else {
                    return false;
                };
                node = object;
            }
            "new_expression" => {
                return node
                    .child_by_field_name("constructor")
                    .is_some_and(|constructor| &source[constructor.byte_range()] == "WebView");
            }
            _ => return false,
        }
    }
}

fn calls_store_true(source: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let mut nodes = vec![tree.root_node()];
    while let Some(node) = nodes.pop() {
        if node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .is_some_and(|function| {
                    function.kind() == "member_expression"
                        && function
                            .child_by_field_name("property")
                            .is_some_and(|property| &source[property.byte_range()] == "store")
                        && function
                            .child_by_field_name("object")
                            .is_some_and(|object| is_webview_chain(object, source))
                })
            && node
                .child_by_field_name("arguments")
                .is_some_and(|arguments| {
                    let mut cursor = arguments.walk();
                    arguments
                        .named_children(&mut cursor)
                        .any(|argument| &source[argument.byte_range()] == "true")
                })
        {
            return true;
        }
        let mut cursor = node.walk();
        nodes.extend(node.children(&mut cursor));
    }
    false
}

fn contributions(value: &Value, declared: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(key.as_str(), "view" | "overlay" | "settings") {
                    if let Some(name) = value.as_str() {
                        declared.push(name.into());
                    }
                } else {
                    contributions(value, declared);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                contributions(value, declared);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn exports(source: &str) -> Result<BTreeSet<String>> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .map_err(|error| OpError::Failed(error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| OpError::Invalid("cannot parse plugin entry".into()))?;
    if tree.root_node().has_error() {
        let mut nodes = vec![tree.root_node()];
        while let Some(node) = nodes.pop() {
            if node.is_error() || node.is_missing() {
                return Err(OpError::Invalid(format!(
                    "invalid JavaScript at line {}",
                    node.start_position().row + 1
                )));
            }
            let mut cursor = node.walk();
            nodes.extend(node.children(&mut cursor));
        }
        return Err(OpError::Invalid("invalid JavaScript".into()));
    }
    let mut exports = BTreeSet::new();
    let mut cursor = tree.root_node().walk();
    for node in tree
        .root_node()
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "export_statement")
    {
        let mut cursor = node.walk();
        if node
            .children(&mut cursor)
            .any(|node| node.kind() == "default")
        {
            exports.insert("default".into());
            continue;
        }
        if let Some(declaration) = node.child_by_field_name("declaration") {
            exported(declaration, source, &mut exports);
        }
        let mut cursor = node.walk();
        for clause in node
            .named_children(&mut cursor)
            .filter(|node| node.kind() == "export_clause")
        {
            let mut cursor = clause.walk();
            for specifier in clause.named_children(&mut cursor) {
                if let Some(name) = specifier
                    .child_by_field_name("alias")
                    .or_else(|| specifier.child_by_field_name("name"))
                {
                    exports.insert(source[name.byte_range()].trim_matches(['\'', '"']).into());
                }
            }
        }
    }
    Ok(exports)
}

fn exported(node: Node<'_>, source: &str, exports: &mut BTreeSet<String>) {
    if let Some(name) = node.child_by_field_name("name") {
        if name.kind() == "identifier" {
            exports.insert(source[name.byte_range()].into());
        }
    } else {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            exported(child, source, exports);
        }
    }
}

pub(crate) fn logs(path: &Path, after: u64) -> Result<OpResult> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(OpResult::Logs {
                text: String::new(),
                after: 0,
                reset: after != 0,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let reset = after > file.metadata()?.len();
    let after = if reset { 0 } else { after };
    file.seek(std::io::SeekFrom::Start(after))?;
    let mut bytes = Vec::new();
    file.take(crate::log::LIMIT).read_to_end(&mut bytes)?;
    let length = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |ix| ix + 1);
    let text = String::from_utf8(bytes[..length].to_owned())
        .map_err(|error| OpError::Failed(error.to_string()))?;
    Ok(OpResult::Logs {
        text,
        after: after + u64::try_from(length).map_err(|error| OpError::Failed(error.to_string()))?,
        reset,
    })
}
