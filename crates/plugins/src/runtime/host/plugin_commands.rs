use kg_domain::PluginCommand;

use super::PluginHost;

/// Every `kg` command of the running plugins, in plugin id and manifest order.
pub(super) fn running(host: &PluginHost) -> Vec<PluginCommand> {
    host.active
        .iter()
        .flat_map(|(id, active)| {
            active.manifest.cli.iter().flat_map(move |group| {
                group.commands.iter().map(move |command| PluginCommand {
                    plugin: id.clone(),
                    group: group.name.clone(),
                    command: command.name.clone(),
                    usage: command.usage.clone(),
                })
            })
        })
        .collect()
}
