use kg_domain::PluginCommand;

const HEAD: &str = "---\nname: kg-plugin-commands\ndescription: Lists the kg commands that enabled plugins add, with their usage. Use when a person asks what a plugin can do from the command line, or before you run a kg command you have not seen.\n---\n\nRun each command as written. The usage is all kagan knows about a plugin command. Each line names the plugin that provides it.\n\n";

/// The generated skill that lists each plugin's `kg` command. None when no plugin adds one.
pub(super) fn skill(commands: &[PluginCommand]) -> Option<String> {
    if commands.is_empty() {
        return None;
    }
    let lines: Vec<String> = commands
        .iter()
        .map(|command| {
            let arguments = command.usage.as_deref().unwrap_or(&command.command);
            format!(
                "- `kg {} {arguments}`, from {}",
                command.group, command.plugin.0
            )
        })
        .collect();
    Some(format!("{HEAD}{}\n", lines.join("\n")))
}
