#[cfg(not(target_os = "linux"))]
use kg_domain::PluginId;

#[cfg(not(target_os = "linux"))]
use crate::PluginHost;

#[cfg(not(target_os = "linux"))]
impl PluginHost {
    #[must_use]
    pub(crate) fn clipboard_write(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.powers.clipboard.write)
    }
}
