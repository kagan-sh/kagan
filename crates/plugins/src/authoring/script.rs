//! Static checks for a script the host throws on when a plugin loads or a panel opens.
//!
//! The manifest check does not run the script. These read the same host declarations
//! `kg plugin types` writes, and the component schema the running app registers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gpui_shell::ArgumentDescriptor;
use kg_domain::{OpError, PluginId};
use tree_sitter::Node;

use crate::Result;

use shape::{
    args_problem, argument_nodes, child_kind, constructor_call, entity_kind, expects, hint,
    is_optional, literal_clash, peel, shown, text, type_only, unquote,
};

mod shape;

pub(crate) fn check(root: &Path, components: &gpui_shell::FrozenComponentRegistry) -> Result<()> {
    let exports = host_exports(root)?;
    let catalog = Catalog::from(components);
    let mut problems = Vec::new();
    for script in super::development::plugin_scripts(root)? {
        let source = std::fs::read_to_string(&script)?;
        let shown = shown(root, &script);
        problems.extend(scan(&source, &shown, &exports, &catalog)?);
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(OpError::Invalid(problems.join("\n")))
    }
}

fn host_exports(root: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let jobs = crate::jobs::Jobs::new();
    let modules = crate::area::modules(
        &PluginId("dev.kagan.types".into()),
        crate::area::Values {
            settings: std::rc::Rc::default(),
            secrets: crate::secret::Store::new(None, root.to_path_buf()),
            status: None,
        },
        &jobs,
        None,
        root,
    );
    let mut exports = BTreeMap::new();
    for (_, module) in modules {
        let body = module
            .declared()
            .ok_or_else(|| OpError::Failed(format!("{} has no declarations", module.name())))?;
        let mut names = BTreeSet::new();
        for line in body.lines() {
            let line = line.trim();
            let Some(rest) = line
                .strip_prefix("export function ")
                .or_else(|| line.strip_prefix("export async function "))
            else {
                continue;
            };
            let name = rest
                .split(['(', ' ', '<'])
                .next()
                .unwrap_or_default()
                .trim();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                names.insert(name.to_owned());
            }
        }
        exports.insert(module.name().to_owned(), names);
    }
    Ok(exports)
}

struct StateFact<'a> {
    kind: &'a str,
    arguments: &'a [ArgumentDescriptor],
}

struct Catalog<'a> {
    module: &'a str,
    states: BTreeMap<&'a str, StateFact<'a>>,
    components: BTreeMap<&'a str, Vec<&'a [ArgumentDescriptor]>>,
}

impl<'a> Catalog<'a> {
    fn from(components: &'a gpui_shell::FrozenComponentRegistry) -> Self {
        let mut states = BTreeMap::new();
        for state in components.states() {
            states.insert(
                state.export(),
                StateFact {
                    kind: state.kind(),
                    arguments: state.arguments(),
                },
            );
        }
        let mut found = BTreeMap::<&str, Vec<&[ArgumentDescriptor]>>::new();
        for descriptor in components.descriptors() {
            let constructors = found.entry(descriptor.name()).or_default();
            for constructor in descriptor.constructors() {
                constructors.push(constructor.arguments());
            }
        }
        Self {
            module: components
                .module_specifier()
                .unwrap_or(gpui_shell::DEFAULT_COMPONENT_MODULE),
            states,
            components: found,
        }
    }
}

#[derive(Clone)]
enum Bound {
    Value { module: String, name: String },
    Namespace { module: String },
}

#[derive(Clone)]
struct Origin {
    module: String,
    kind: String,
}

struct Scan<'a> {
    source: &'a str,
    shown: &'a str,
    exports: &'a BTreeMap<String, BTreeSet<String>>,
    catalog: &'a Catalog<'a>,
    bindings: BTreeMap<String, Bound>,
    origins: BTreeMap<String, Origin>,
    problems: Vec<String>,
}

fn scan(
    source: &str,
    shown: &str,
    exports: &BTreeMap<String, BTreeSet<String>>,
    catalog: &Catalog<'_>,
) -> Result<Vec<String>> {
    let mut parser = super::development::javascript_parser()
        .map_err(|error| OpError::Failed(error.to_string()))?;
    let Some(tree) = parser.parse(source, None) else {
        return Ok(Vec::new());
    };
    let mut scan = Scan {
        source,
        shown,
        exports,
        catalog,
        bindings: BTreeMap::new(),
        origins: BTreeMap::new(),
        problems: Vec::new(),
    };
    scan.collect_imports(tree.root_node());
    scan.visit(tree.root_node());
    Ok(scan.problems)
}

impl Scan<'_> {
    fn collect_imports(&mut self, node: Node<'_>) {
        if node.kind() == "import_statement" {
            self.import(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_imports(child);
        }
    }

    fn import(&mut self, node: Node<'_>) {
        let Some(source) = node.child_by_field_name("source") else {
            return;
        };
        let Some(module) = unquote(text(source, self.source)) else {
            return;
        };
        if type_only(text(node, self.source)) {
            return;
        }
        let Some(clause) = child_kind(node, "import_clause") else {
            return;
        };
        if let Some(namespace) = child_kind(clause, "namespace_import")
            && let Some(name) = child_kind(namespace, "identifier")
        {
            self.bindings.insert(
                text(name, self.source).to_owned(),
                Bound::Namespace {
                    module: module.to_owned(),
                },
            );
        }
        let Some(named) = child_kind(clause, "named_imports") else {
            return;
        };
        let mut cursor = named.walk();
        for specifier in named.children(&mut cursor) {
            if specifier.kind() != "import_specifier" || type_only(text(specifier, self.source)) {
                continue;
            }
            let Some(exported) = specifier.child_by_field_name("name") else {
                continue;
            };
            let export = match exported.kind() {
                "string" => match unquote(text(exported, self.source)) {
                    Some(name) => name.to_owned(),
                    None => continue,
                },
                _ => text(exported, self.source).to_owned(),
            };
            let local = specifier.child_by_field_name("alias").map_or_else(
                || export.clone(),
                |alias| text(alias, self.source).to_owned(),
            );
            if let Some(problem) = self.missing_export(module, &export) {
                self.problems.push(problem);
            }
            self.bindings.insert(
                local,
                Bound::Value {
                    module: module.to_owned(),
                    name: export,
                },
            );
        }
    }

    fn missing_export(&self, module: &str, name: &str) -> Option<String> {
        if !module.starts_with("kagan/") {
            return None;
        }
        match self.exports.get(module) {
            None => Some(format!(
                "{shown} imports {name} from \"{module}\", which is not a host module",
                shown = self.shown
            )),
            Some(names) if !names.contains(name) => Some(format!(
                "{shown} imports {name} from \"{module}\", which does not export it. kg plugin types writes the exports in kagan.d.ts",
                shown = self.shown
            )),
            Some(_) => None,
        }
    }

    fn visit(&mut self, node: Node<'_>) {
        self.record_origin(node);
        self.note_namespace(node);
        self.note_call(node);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child);
        }
    }

    fn record_origin(&mut self, node: Node<'_>) {
        let (left, right) = match node.kind() {
            "variable_declarator" => (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
            ),
            "assignment_expression" => (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ),
            _ => return,
        };
        let (Some(left), Some(right)) = (left, right) else {
            return;
        };
        if left.kind() == "object_pattern" || left.kind() == "array_pattern" {
            return;
        }
        let Some(origin) = self.origin_of(right) else {
            return;
        };
        self.origins
            .insert(text(left, self.source).to_owned(), origin);
    }

    fn note_namespace(&mut self, node: Node<'_>) {
        if node.kind() != "member_expression" {
            return;
        }
        let Some(object) = node.child_by_field_name("object") else {
            return;
        };
        if object.kind() != "identifier" {
            return;
        }
        let Some(Bound::Namespace { module }) =
            self.bindings.get(text(object, self.source)).cloned()
        else {
            return;
        };
        let Some(property) = node.child_by_field_name("property") else {
            return;
        };
        if property.kind() == "private_property_identifier" {
            return;
        }
        let name = text(property, self.source);
        if let Some(problem) = self.missing_export(&module, name) {
            self.problems.push(problem.replace(
                &format!("imports {name} from"),
                &format!("reads {name} from"),
            ));
        }
    }

    fn note_call(&mut self, node: Node<'_>) {
        let Some(call) = constructor_call(node, self.source) else {
            return;
        };
        let Some(Bound::Value { module, name }) = self.bindings.get(&call.local).cloned() else {
            return;
        };
        if module != self.catalog.module {
            return;
        }
        let arguments = argument_nodes(call.node);
        if let Some(state) = self.catalog.states.get(name.as_str())
            && let Some(problem) = args_problem(state.arguments, &arguments)
        {
            self.problems.push(format!(
                "{shown}: {name}.new from \"{module}\" {problem}.{hint}",
                shown = self.shown,
                hint = hint(&name, &problem),
            ));
        }
        if let Some(constructors) = self.catalog.components.get(name.as_str()) {
            let mut failures = Vec::new();
            for parameters in constructors {
                match self.constructor_problem(&name, parameters, &arguments) {
                    None => return,
                    Some(failure) => failures.push(failure),
                }
            }
            if let Some(failure) = failures.into_iter().next() {
                self.problems.push(failure);
            }
        }
    }

    fn constructor_problem(
        &self,
        component: &str,
        parameters: &[ArgumentDescriptor],
        arguments: &[Node<'_>],
    ) -> Option<String> {
        if arguments.len() > parameters.len() {
            return Some(format!(
                "{shown}: {component} got {} arguments; it takes {}",
                arguments.len(),
                parameters.len(),
                shown = self.shown
            ));
        }
        for (parameter, argument) in parameters.iter().zip(arguments.iter()) {
            if let Some(got) = literal_clash(parameter.schema(), *argument) {
                return Some(format!(
                    "{shown}: {component}() argument {} expects {}, not {got}",
                    parameter.name(),
                    expects(parameter.schema()),
                    shown = self.shown
                ));
            }
            if let Some(kind) = entity_kind(parameter.schema())
                && let Some(problem) = self.bad_entity(component, parameter.name(), kind, *argument)
            {
                return Some(problem);
            }
        }
        for parameter in parameters.iter().skip(arguments.len()) {
            if !is_optional(parameter.schema()) {
                return Some(format!(
                    "{shown}: {component}() argument {} is required",
                    parameter.name(),
                    shown = self.shown
                ));
            }
        }
        None
    }

    fn bad_entity(
        &self,
        component: &str,
        argument: &str,
        kind: &str,
        node: Node<'_>,
    ) -> Option<String> {
        let module = self.catalog.module;
        let shape =
            if matches!(kind, "TextareaState" | "InputState") || peel(node).kind() == "object" {
                hint(kind, "object")
            } else {
                ""
            };
        if matches!(
            peel(node).kind(),
            "object" | "array" | "string" | "template_string" | "number" | "true" | "false"
        ) {
            return Some(format!(
                "{shown}: {component}({argument}) expects the {kind} from \"{module}\", created with {kind}.new(...) imported from \"{module}\".{shape}",
                shown = self.shown
            ));
        }
        let origin = self.origin_of(node)?;
        if origin.module == module && origin.kind == kind {
            return None;
        }
        let found = if origin.kind == kind {
            format!("this one is {}'s", origin.module)
        } else {
            format!("this one is {}'s {}", origin.module, origin.kind)
        };
        Some(format!(
            "{shown}: {component}({argument}) expects the {kind} from \"{module}\", created with {kind}.new(...) imported from \"{module}\"; {found}.{shape}",
            shown = self.shown
        ))
    }

    fn origin_of(&self, node: Node<'_>) -> Option<Origin> {
        let node = peel(node);
        if let Some(call) = constructor_call(node, self.source) {
            let Bound::Value { module, name } = self.bindings.get(&call.local)? else {
                return None;
            };
            let state = self.catalog.states.get(name.as_str())?;
            return Some(Origin {
                module: module.clone(),
                kind: state.kind.to_owned(),
            });
        }
        self.origins.get(text(node, self.source)).cloned()
    }
}
