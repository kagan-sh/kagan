use gpui_kit::TestAppContext;

use super::{
    assert_folder_write_absent, assert_folder_write_landed, folder_plugin_fixture,
    queue_folder_write,
};

#[test]
fn plugin_folders_sit_under_the_data_folder() {
    use std::path::PathBuf;
    let data = PathBuf::from("data");
    let id = kg_domain::PluginId("dev.test.echo".into());
    assert_eq!(
        crate::plugin_folder::plugins_dir(&data),
        data.join("plugins")
    );
    let dir = data.join("plugins").join("dev.test.echo");
    assert_eq!(crate::plugin_folder::plugin_dir(&data, &id), dir);
    assert_eq!(
        crate::plugin_folder::plugin_data_dir(&data, &id),
        dir.join("data")
    );
    assert_eq!(
        crate::plugin_folder::plugin_log_file(&data, &id),
        dir.join("logs.jsonl")
    );
}

#[gpui_kit::test]
async fn a_queued_plugin_write_is_refused_once_its_permit_is_cleared(cx: &mut TestAppContext) {
    let fixture = folder_plugin_fixture(cx).await;
    let permit = std::sync::Arc::new(std::sync::Mutex::new(Some(1)));
    let mut write = std::pin::pin!(queue_folder_write(cx, &fixture, permit.clone()));
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    assert!(write.as_mut().poll(&mut context).is_pending());
    *permit.lock().unwrap() = None;
    assert!(cx.foreground_executor().block_test(write).is_err());
    assert_folder_write_absent(&fixture);
}

#[gpui_kit::test]
async fn a_queued_plugin_write_lands_while_its_permit_holds(cx: &mut TestAppContext) {
    let fixture = folder_plugin_fixture(cx).await;
    let permit = std::sync::Arc::new(std::sync::Mutex::new(Some(1)));
    let mut write = std::pin::pin!(queue_folder_write(cx, &fixture, permit));
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    assert!(write.as_mut().poll(&mut context).is_pending());
    cx.foreground_executor().block_test(write).unwrap();
    assert_folder_write_landed(&fixture);
}
