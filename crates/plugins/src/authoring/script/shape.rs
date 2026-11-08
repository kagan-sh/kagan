use gpui_shell::{ArgumentDescriptor, ArgumentSchema};
use tree_sitter::Node;

pub(super) struct Call<'tree> {
    pub(super) local: String,
    pub(super) node: Node<'tree>,
}

pub(super) fn constructor_call<'tree>(node: Node<'tree>, source: &str) -> Option<Call<'tree>> {
    let node = peel(node);
    match node.kind() {
        "call_expression" => {
            let function = node.child_by_field_name("function")?;
            let local = match function.kind() {
                "identifier" => text(function, source).to_owned(),
                "member_expression" => {
                    let property = function.child_by_field_name("property")?;
                    if text(property, source) != "new" {
                        return None;
                    }
                    let object = function.child_by_field_name("object")?;
                    if object.kind() != "identifier" {
                        return None;
                    }
                    text(object, source).to_owned()
                }
                _ => return None,
            };
            Some(Call { local, node })
        }
        "new_expression" => {
            let constructor = node.child_by_field_name("constructor")?;
            if constructor.kind() != "identifier" {
                return None;
            }
            Some(Call {
                local: text(constructor, source).to_owned(),
                node,
            })
        }
        _ => None,
    }
}

pub(super) fn argument_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    node.child_by_field_name("arguments")
        .map(|arguments| {
            let mut cursor = arguments.walk();
            arguments.named_children(&mut cursor).collect()
        })
        .unwrap_or_default()
}

pub(super) fn args_problem(
    parameters: &[ArgumentDescriptor],
    arguments: &[Node<'_>],
) -> Option<String> {
    if arguments.len() > parameters.len() {
        return Some(format!(
            "got {} arguments; it takes {}",
            arguments.len(),
            parameters.len()
        ));
    }
    for (parameter, argument) in parameters.iter().zip(arguments.iter()) {
        if let Some(got) = literal_clash(parameter.schema(), *argument) {
            return Some(format!(
                "argument {} expects {}, not {got}",
                parameter.name(),
                expects(parameter.schema())
            ));
        }
    }
    for parameter in parameters.iter().skip(arguments.len()) {
        if !is_optional(parameter.schema()) {
            return Some(format!("argument {} is required", parameter.name()));
        }
    }
    None
}

pub(super) fn literal_clash(schema: &ArgumentSchema, node: Node<'_>) -> Option<&'static str> {
    let node = peel(node);
    match schema {
        ArgumentSchema::Optional(inner) => {
            if node.kind() == "null" {
                None
            } else {
                literal_clash(inner, node)
            }
        }
        ArgumentSchema::String | ArgumentSchema::Enum(_) => clash(node, "a string"),
        ArgumentSchema::Number => clash(node, "a number"),
        ArgumentSchema::Boolean => clash(node, "a boolean"),
        _ => None,
    }
}

fn clash(node: Node<'_>, expect: &'static str) -> Option<&'static str> {
    let got = match node.kind() {
        "object" => "an object",
        "array" => "an array",
        "string" | "template_string" => "a string",
        "number" => "a number",
        "true" | "false" => "a boolean",
        _ => return None,
    };
    (got != expect).then_some(got)
}

pub(super) fn entity_kind(schema: &ArgumentSchema) -> Option<&str> {
    match schema {
        ArgumentSchema::Entity(kind) => Some(kind),
        ArgumentSchema::Optional(inner) => entity_kind(inner),
        _ => None,
    }
}

pub(super) fn is_optional(schema: &ArgumentSchema) -> bool {
    matches!(schema, ArgumentSchema::Optional(_))
}

pub(super) fn expects(schema: &ArgumentSchema) -> &'static str {
    match schema {
        ArgumentSchema::Optional(inner) => expects(inner),
        ArgumentSchema::String | ArgumentSchema::Enum(_) => "a string",
        ArgumentSchema::Number => "a number",
        ArgumentSchema::Boolean => "a boolean",
        _ => "the documented value",
    }
}

pub(super) fn hint(export: &str, problem: &str) -> &'static str {
    if !problem.contains("object") {
        return "";
    }
    match export {
        "TextareaState" => {
            " TextareaState.new takes one optional string, the initial value. gpui-base's TextareaState.new({ placeholder, value, rows }) is a different constructor, and KgTextarea rejects that state"
        }
        "InputState" => {
            " InputState.new takes two optional strings, placeholder then the initial value. gpui-base's InputState.new({ placeholder, value }) is a different constructor, and KgInput rejects that state"
        }
        _ => " An options object is gpui-base's constructor of the same name",
    }
}

pub(super) fn peel(node: Node<'_>) -> Node<'_> {
    let mut node = node;
    while node.kind() == "parenthesized_expression" {
        let Some(inner) = node.named_child(0) else {
            break;
        };
        node = inner;
    }
    node
}

pub(super) fn type_only(text: &str) -> bool {
    let rest = text.trim_start();
    let rest = rest.strip_prefix("import").map_or(rest, str::trim_start);
    rest.starts_with("type ") || rest.starts_with("type{") || rest.starts_with("type\n")
}

pub(super) fn child_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == kind)
}

pub(super) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

pub(super) fn unquote(literal: &str) -> Option<&str> {
    let bytes = literal.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[0] == bytes[bytes.len() - 1]
    {
        Some(&literal[1..literal.len() - 1])
    } else {
        None
    }
}

pub(super) fn shown(root: &std::path::Path, script: &std::path::Path) -> String {
    script
        .strip_prefix(root)
        .unwrap_or(script)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
