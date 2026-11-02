use kg_domain::PluginId;

use crate::PluginHost;

impl PluginHost {
    #[must_use]
    pub(crate) fn clipboard_write(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.powers.clipboard.write)
    }
}
