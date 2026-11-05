use std::time::Duration;

use gpui_shell::RuntimeMetrics;

const BUDGET: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(crate) struct Watchdog {
    metrics: RuntimeMetrics,
    repeated: u64,
    pub(crate) slow: bool,
    pub(crate) error: Option<String>,
}

impl Watchdog {
    pub(crate) fn sample(
        &mut self,
        metrics: RuntimeMetrics,
        error: Option<&str>,
    ) -> Option<String> {
        let elapsed = metrics.since(&self.metrics);
        self.metrics = metrics;
        if let Some(error) = error
            && self.error.as_deref() != Some(error)
        {
            self.error = Some(error.to_owned());
            return Some(format!("crashed: {error}"));
        }
        if elapsed.script_renders() == 0 {
            return None;
        }
        if elapsed.mean_script_render() > BUDGET {
            self.repeated += elapsed.script_renders();
        } else {
            self.repeated = 0;
        }
        if self.repeated >= 3 && !self.slow {
            self.slow = true;
            return Some("slow: repeated script renders average more than 16 ms; turn the plugin off in Settings → Installed".into());
        }
        None
    }
}

#[derive(Clone)]
pub struct PluginStatus {
    /// This plugin's id.
    pub id: kg_domain::PluginId,
    /// The name its manifest gives it.
    pub name: String,
    /// `None` when the plugin's manifest could not be read.
    pub version: Option<String>,
    pub source: String,
    /// What the person approved, one line each.
    pub powers: Vec<String>,
    /// Whether the person has it enabled.
    pub enabled: bool,
    /// Whether it has been slow enough to warn about.
    pub slow: bool,
    /// Why it failed to load, when it did.
    pub error: Option<String>,
    /// What the person must know about it, when there is anything.
    pub note: Option<String>,
}
