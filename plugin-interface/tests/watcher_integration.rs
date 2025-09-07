#![cfg(feature = "watch")]

use plugin_interface::{PluginManager, PluginTrait, WatchOptions};
use std::fs;
use std::path::PathBuf;

#[test]
fn watcher_auto_loads_new_plugin() {
    // Create a temp directory
    let tmpdir = tempfile::tempdir().expect("tmpdir");
    let dir = tmpdir.path().to_path_buf();

    // Find an existing built plugin artifact to copy into the temp dir.
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
            "watcher_integration test: plugin artifact not found at {:?}, skipping",
            candidate
        );
        return;
    }

    let mut mgr = PluginManager::new();

    // Run watcher in a small helper thread: when the callback receives handles,
    // record that and return false to stop watching.
    let opts = WatchOptions {
        debounce_ms: 200,
        recursive: false,
        auto_load: true,
        auto_unload: false,
        emit_proxies: false,
    };

    // Copy the plugin into the temp dir after starting the watcher in another
    // thread so the watcher will observe the new file.
    let copy_path = candidate.clone();
    let dir_clone = dir.clone();

    let saw_handles = std::sync::Arc::new(std::sync::Mutex::new(false));
    let saw_handles_clone = saw_handles.clone();

    std::thread::spawn(move || {
        // small sleep to ensure watcher started
        std::thread::sleep(std::time::Duration::from_millis(150));
        let dest = dir_clone.join(copy_path.file_name().unwrap());
        let _ = fs::copy(&copy_path, &dest).expect("copy plugin");
    });

    mgr.watch_and_load_blocking(dir, PluginTrait::Greeter, opts, move |evt| {
        match evt {
            plugin_interface::WatchEvent::Handles(handles, _paths) => {
                if !handles.is_empty() {
                    let mut locked = saw_handles_clone.lock().unwrap();
                    *locked = true;
                    return false; // stop watching
                }
            }
            plugin_interface::WatchEvent::Proxies(proxies, _paths) => {
                if !proxies.is_empty() {
                    let mut locked = saw_handles_clone.lock().unwrap();
                    *locked = true;
                    return false;
                }
            }
        }
        true
    });

    let locked = saw_handles.lock().unwrap();
    assert!(*locked, "watcher did not report loaded handles");
}
