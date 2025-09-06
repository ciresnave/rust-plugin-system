#![cfg(feature = "watch")]

use plugin_interface::{ManagerNotification, PluginManager, PluginTrait, WatchEvent, WatchOptions};
use std::fs;
use std::path::PathBuf;

#[test]
fn manager_background_watcher_loads_plugins() {
    // temp dir
    let tmpdir = tempfile::tempdir().expect("tmpdir");
    let dir = tmpdir.path().to_path_buf();

    // Find build artifact to copy
    let mut candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidate.push("../../plugins/plugin-multi/target/debug");

    #[cfg(target_os = "windows")]
    candidate.push("plugin_multi.dll");
    #[cfg(target_os = "macos")]
    candidate.push("libplugin_multi.dylib");
    #[cfg(all(unix, not(target_os = "macos")))]
    candidate.push("libplugin_multi.so");

    if !candidate.exists() {
        eprintln!(
            "manager_integration test: plugin artifact not found at {:?}, skipping",
            candidate
        );
        return;
    }

    let mut mgr = PluginManager::new();

    let opts_bg = WatchOptions {
        debounce_ms: 200,
        recursive: false,
        auto_load: true,
        auto_unload: false,
        emit_proxies: false,
    };

    // start background watcher (emits conservative WatchNotification)
    let (rx, stop_tx, handle) = mgr.start_watch_background(dir.clone(), opts_bg);

    // spawn copier thread to add the plugin after a short delay
    let copy_path = candidate.clone();
    let dir_clone = dir.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let dest = dir_clone.join(copy_path.file_name().unwrap());
        let _ = fs::copy(&copy_path, &dest).expect("copy plugin");
    });

    // process notifications on manager-owned thread; stop when we see handles
    let opts_proc = WatchOptions {
        debounce_ms: 200,
        recursive: false,
        auto_load: true,
        auto_unload: false,
        emit_proxies: false,
    };

    let mut saw = false;
    mgr.process_watch_notifications_blocking(&dir, rx, PluginTrait::Greeter, opts_proc, |not| {
        match not {
            ManagerNotification::Event(WatchEvent::Handles(handles, _paths)) => {
                if !handles.is_empty() {
                    saw = true;
                    return false; // stop processing
                }
            }
            ManagerNotification::Event(WatchEvent::Proxies(proxies, _paths)) => {
                if !proxies.is_empty() {
                    saw = true;
                    return false;
                }
            }
            _ => {}
        }
        true
    });

    // stop background watcher and join
    let _ = stop_tx.send(());
    let _ = handle.join();

    assert!(saw, "manager background watcher did not load plugins");
}
