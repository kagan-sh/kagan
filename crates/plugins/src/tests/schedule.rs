use std::path::Path;
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{OpResult, Plugin, PluginId};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::PluginHostGlobal;
use crate::manifest::{self, Grant};

use super::{open_kept, read_manifest};

const SCHEDULED: &str = "dev.test.schedule";

const SCHEDULE_MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, fail } from "kagan/jobs";

let runs = 0;

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        runs += 1;
        // The first run never answers, so the host has to time it out.
        if (runs > 1) fail(job.id, `run ${runs} ${job.name}`);
      }
    });
  }
  render() {
    return "schedule";
  }
}
"#;

const SCHEDULE_THROWS: &str = r#"
import { View } from "gpui-kit";
import { next } from "kagan/jobs";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        // The loop dies here, so nothing answers the job.
        throw new Error(`run ${job.name} threw`);
      }
    });
  }
  render() {
    return "schedule";
  }
}
"#;

fn scheduled_plugin(schedules: &Value) -> TempDir {
    scheduled_plugin_with(schedules, SCHEDULE_MAIN)
}

fn scheduled_plugin_with(schedules: &Value, main: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let manifest = json!({
        "id": SCHEDULED,
        "name": "Schedule",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "schedules": schedules,
    });
    std::fs::write(dir.path().join(manifest::FILE), manifest.to_string()).unwrap();
    std::fs::write(dir.path().join("main.js"), main).unwrap();
    dir
}

fn scheduled(dir: &Path) -> Plugin {
    let root = dunce::canonicalize(dir).unwrap();
    Plugin {
        id: PluginId(SCHEDULED.into()),
        source: root.display().to_string(),
        approved: serde_json::to_value(Grant::of(&read_manifest(&root).unwrap(), &root)).unwrap(),
        root,
        commit: None,
        enabled: true,
        error: None,
    }
}

fn scheduled_loaded(cx: &mut TestAppContext) -> bool {
    cx.read(|cx| {
        cx.global::<PluginHostGlobal>()
            .0
            .read(cx)
            .active
            .get(&PluginId(SCHEDULED.into()))
            .is_some_and(|active| active.loaded.is_some())
    })
}

fn scheduled_generation(cx: &mut TestAppContext) -> u64 {
    cx.read(|cx| {
        cx.global::<PluginHostGlobal>()
            .0
            .read(cx)
            .active
            .get(&PluginId(SCHEDULED.into()))
            .map_or(0, |active| active.generation)
    })
}

fn plugin_log(data: &Path) -> String {
    let file = data.join("plugins").join(SCHEDULED).join("logs.jsonl");
    if !file.exists() {
        return String::new();
    }
    let OpResult::Logs { text, .. } = crate::development::logs(&file, 0).unwrap() else {
        panic!("logs")
    };
    text
}

#[gpui_kit::test]
fn a_schedule_wakes_the_plugin_once_per_interval_and_waits_for_each_run(cx: &mut TestAppContext) {
    let plugin = scheduled_plugin(&json!([{ "id": "tick", "every": 1 }]));
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[scheduled(plugin.path())]);
    cx.run_until_parked();
    assert!(!scheduled_loaded(cx), "a schedule waits one interval");

    cx.executor().advance_clock(Duration::from_mins(1));
    cx.run_until_parked();
    assert!(scheduled_loaded(cx));
    assert_eq!(plugin_log(&path), "");

    // Run one never answers, so the next interval starts no second run.
    cx.executor().advance_clock(Duration::from_mins(1));
    cx.run_until_parked();
    assert!(!plugin_log(&path).contains("run"), "{}", plugin_log(&path));

    // Run one gives up after two minutes, and its load is retired with it.
    cx.executor().advance_clock(Duration::from_mins(1));
    cx.run_until_parked();
    let log = plugin_log(&path);
    assert!(log.contains("did not finish within 120 seconds"), "{log}");
    assert!(!scheduled_loaded(cx));

    // The wait after it starts the next run on a new load.
    cx.executor().advance_clock(Duration::from_mins(1));
    cx.run_until_parked();
    assert!(scheduled_loaded(cx));
    assert_eq!(scheduled_generation(cx), 2);
}

#[gpui_kit::test]
fn a_run_that_throws_retires_its_load_so_the_next_run_starts_a_new_one(cx: &mut TestAppContext) {
    let plugin = scheduled_plugin_with(&json!([{ "id": "tick", "every": 1 }]), SCHEDULE_THROWS);
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[scheduled(plugin.path())]);
    cx.run_until_parked();

    cx.executor().advance_clock(Duration::from_mins(1));
    cx.run_until_parked();
    assert!(scheduled_loaded(cx));
    assert_eq!(scheduled_generation(cx), 1);

    // The loop threw, so nothing answers; the run gives up and its load is retired.
    cx.executor().advance_clock(Duration::from_mins(2));
    cx.run_until_parked();
    let log = plugin_log(&path);
    assert!(log.contains("did not finish within 120 seconds"), "{log}");
    assert!(!scheduled_loaded(cx), "a timed-out load is not kept");

    // The next run loads the plugin again instead of reusing the dead queue.
    cx.executor().advance_clock(Duration::from_mins(1));
    cx.run_until_parked();
    assert!(scheduled_loaded(cx), "the next run loads the plugin");
    assert_eq!(scheduled_generation(cx), 2);
}

#[gpui_kit::test]
fn disabling_a_scheduled_plugin_stops_its_clock(cx: &mut TestAppContext) {
    let plugin = scheduled_plugin(&json!([{ "id": "tick", "every": 1 }]));
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[scheduled(plugin.path())]);
    cx.run_until_parked();
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, cx| {
            host.disable_loaded(&PluginId(SCHEDULED.into()), cx);
        });
    });
    cx.executor().advance_clock(Duration::from_mins(5));
    cx.run_until_parked();
    assert!(!scheduled_loaded(cx));
    assert_eq!(plugin_log(&path), "");
}

#[test]
fn schedules_need_a_name_and_whole_minutes_up_to_a_day() {
    for schedules in [
        json!([{ "id": "tick", "every": 0 }]),
        json!([{ "id": "tick", "every": 1441 }]),
        json!([{ "id": "", "every": 5 }]),
        json!([{ "id": "tick", "every": 5 }, { "id": "tick", "every": 6 }]),
        json!([{ "id": "tick", "every": 5, "unknown": true }]),
    ] {
        let plugin = scheduled_plugin(&schedules);
        assert!(read_manifest(plugin.path()).is_err(), "{schedules}");
    }
    let plugin = scheduled_plugin(&json!([{ "id": "tick", "every": 1440 }]));
    assert!(read_manifest(plugin.path()).is_ok());
}

#[test]
fn a_schedule_is_an_install_line_and_needs_approval_again_when_it_changes() {
    let plugin = scheduled_plugin(&json!([{ "id": "tick", "every": 5 }]));
    let asked = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        asked
            .lines()
            .contains(&"runs on its own every 5 minutes while kagan is open".to_owned()),
        "{:?}",
        asked.lines()
    );
    assert!(asked.within(&asked));
    assert!(!asked.within(&Grant::default()));
    let mut faster = asked.clone();
    faster.schedules.insert("tick".into(), 1);
    assert!(!faster.within(&asked));
    assert!(!asked.within(&faster));

    assert_eq!(asked.intersect(&asked).schedules, asked.schedules);
    assert!(asked.intersect(&faster).schedules.is_empty());
    assert!(asked.intersect(&Grant::default()).schedules.is_empty());

    let mut mine = Grant::default();
    mine.schedules.insert("tick".into(), 5);
    mine.schedules.insert("gone".into(), 7);
    let mut theirs = Grant::default();
    theirs.schedules.insert("tick".into(), 5);
    theirs.schedules.insert("new".into(), 9);
    assert_eq!(
        mine.intersect(&theirs).schedules,
        std::collections::BTreeMap::from([("tick".to_owned(), 5)])
    );
}
