use super::*;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn test_path(label: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/settings-tests")
        .join(format!(
            "{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&root).unwrap();
    root.join("settings.json")
}
fn wait_until(mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "settings operation timed out"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn updates_preserve_other_plugins_unknown_keys_and_reject_damaged_or_future_files() {
    let path = test_path("preservation");
    let store = Store::new(path.clone());
    let mut doc = Document::default();
    doc.0["future_metadata"] = json!({"keep": [1, 2, 3]});
    fs::write(&path, serde_json::to_vec(&doc.0).unwrap()).unwrap();
    store
        .update(&[Edit::plugin("first", "scale", json!(1.5))])
        .unwrap();
    let doc = store
        .update(&[
            Edit::global("language", json!("zh-CN")),
            Edit::plugin("second", "theme", json!("dark")),
        ])
        .unwrap();
    assert_eq!(doc.0["plugins"]["first"]["scale"], 1.5);
    assert_eq!(doc.0["future_metadata"]["keep"], json!([1, 2, 3]));
    assert_eq!(doc.language(), "zh-CN");
    for bytes in [
        b"{broken".as_slice(),
        br#"{"schema_version":2,"global":{},"plugins":{}}"#,
        br#"{"schema_version":1,"global":{},"plugins":{"other":3}}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(
            store
                .update(&[Edit::global("maximum_ui_fps", json!(120))])
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn shared_clients_save_latest_edits_and_follow_external_changes() {
    let path = test_path("clients");
    let first = Preferences::open(path.clone()).unwrap();
    let second = Preferences::open(path.clone()).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    wait_until(|| first.snapshot().loaded);
    for fps in [30, 90, 60, 120] {
        first.set(Edit::global("maximum_ui_fps", json!(fps)));
    }
    second.set(Edit::global("language", json!("zh-CN")));
    wait_until(|| !first.snapshot().pending);
    let store = Store::new(path.clone());
    assert_eq!(store.read().unwrap().maximum_ui_fps(), 120);
    assert_eq!(store.read().unwrap().language(), "zh-CN");
    store
        .update(&[Edit::global("maximum_ui_fps", json!(30))])
        .unwrap();
    wait_until(|| second.snapshot().maximum_ui_fps == 30);
    drop(first);
    second.set(Edit::global("maximum_ui_fps", json!(90)));
    drop(second); // Joins and flushes the last edit before library unload.
    assert_eq!(store.read().unwrap().maximum_ui_fps(), 90);
    let reopened = Preferences::open(path).unwrap();
    wait_until(|| reopened.snapshot().loaded);
    assert_eq!(reopened.snapshot().maximum_ui_fps, 90);
}

#[test]
fn independent_process_writers_merge_without_partial_json_or_lost_fields() {
    let path = test_path("processes");
    let store = Store::new(path.clone());
    store.update(&[]).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut children: Vec<_> = (0..4)
        .map(|i| {
            Command::new(&exe)
                .args(["--exact", "tests::child_process", "--ignored"])
                .env("SETTINGS_TEST_PATH", &path)
                .env("SETTINGS_TEST_WRITER", i.to_string())
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    wait_until(|| {
        assert!(store.read().is_ok(), "reader saw a partial JSON file");
        children
            .iter_mut()
            .all(|child| child.try_wait().unwrap().is_some())
    });
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let doc = store.read().unwrap();
    for i in 0..4 {
        let values = &doc.0["plugins"][format!("plugin-{i}")];
        for n in 0..30 {
            assert_eq!(values[format!("setting-{n}")], n);
        }
        assert_eq!(doc.0["global"][format!("writer-{i}")], 29);
    }
}

#[test]
fn os_releases_crashed_writer_lock_and_pending_client_recovers() {
    let path = test_path("crash");
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::child_process", "--ignored"])
            .env("SETTINGS_TEST_PATH", &path)
            .env("SETTINGS_TEST_WRITER", "hold")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_until(|| path.with_extension("ready").exists());
    let store = Store::new(path.clone());
    assert!(matches!(store.update(&[]), Err(Error::Busy)));
    let client = Preferences::open(path).unwrap();
    client.set(Edit::global("maximum_ui_fps", json!(120)));
    wait_until(|| client.snapshot().error.is_some());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    wait_until(|| !client.snapshot().pending);
    assert!(client.snapshot().error.is_none());
    assert_eq!(store.read().unwrap().maximum_ui_fps(), 120);
}

#[test]
#[ignore = "helper process launched by concurrency tests"]
fn child_process() {
    let path = PathBuf::from(std::env::var_os("SETTINGS_TEST_PATH").unwrap());
    let writer = std::env::var("SETTINGS_TEST_WRITER").unwrap();
    if writer == "hold" {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))
            .unwrap();
        file.lock().unwrap();
        fs::write(path.with_extension("ready"), b"locked").unwrap();
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    let store = Store::new(path);
    for n in 0..30 {
        let edits = [
            Edit::plugin(
                &format!("plugin-{writer}"),
                &format!("setting-{n}"),
                json!(n),
            ),
            Edit::global(&format!("writer-{writer}"), json!(n)),
        ];
        wait_until(|| match store.update(&edits) {
            Ok(_) => true,
            Err(Error::Busy) => false,
            Err(e) => panic!("{e}"),
        });
    }
}
