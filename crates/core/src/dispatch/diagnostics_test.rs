use std::path::Path;

use kg_domain::{LogCounts, Plugin, PluginId, PluginLog};

use super::{Facts, render};

fn plugin(id: &str) -> Plugin {
    Plugin {
        id: PluginId(id.into()),
        source: "/plugins/x".into(),
        approved: serde_json::json!({"powers": {}, "downloads": {}}),
        root: Path::new("/plugins/x").into(),
        commit: None,
        enabled: true,
        skills: Vec::new(),
        error: None,
    }
}

#[test]
fn each_plugin_shows_its_own_log_counts() {
    let facts = Facts {
        schema: 1,
        threads: Vec::new(),
        endings: Vec::new(),
        screens: Vec::new(),
        plugins: vec![plugin("a"), plugin("b")],
    };
    let logs = [PluginLog {
        id: PluginId("b".into()),
        counts: LogCounts::Counted { warn: 2, error: 1 },
    }];
    let text = render(&facts, Path::new("/data"), &logs);
    let shown: Vec<&str> = text.lines().filter(|line| line.contains("log:")).collect();
    assert_eq!(
        shown,
        ["  log: not read, no plugin host", "  log: 2 warn, 1 error"],
        "{text}"
    );
}
