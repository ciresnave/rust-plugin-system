use plugin_interface::{PluginManager, PluginTrait};
use std::path::PathBuf;

// This test expects a plugin that exports the unmaker counter getter. If the
// plugin artifact isn't present (for example when running on CI without
// building the example plugins), the test will return early.
#[test]
fn close_returns_unmaker_counter_when_final_owner() {
    // Attempt to locate the example plugin built in the workspace. This mirrors
    // logic in manager_integration.rs but is defensive.
    let mut candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidate.push("../../plugins/plugin-multi/target/debug");

    // platform-specific filename
    #[cfg(target_os = "windows")]
    candidate.push("plugin_multi.dll");
    #[cfg(target_os = "macos")]
    candidate.push("libplugin_multi.dylib");
    #[cfg(all(unix, not(target_os = "macos")))]
    candidate.push("libplugin_multi.so");

    if !candidate.exists() {
        eprintln!(
            "close_counter test: plugin artifact not found at {:?}, skipping",
            candidate
        );
        return;
    }

    let mut mgr = PluginManager::new();
    let dir = candidate.parent().unwrap();
    let handles = match mgr.load_plugins(dir, PluginTrait::Greeter) {
        Ok(h) => h,
        Err(e) => {
            panic!("failed to load plugins: {:?}", e);
        }
    };

    // We expect at least one handle. Take the first, ensure it's the unique
    // owner by dropping all other handles/clones, then call close and assert
    // the returned counter is Some(u64).
    assert!(!handles.is_empty());
    let mut first = handles.into_iter();
    let h = first.next().unwrap();

    // Drop remaining handles so `h` is the last owner.
    drop(first);

    match h.close() {
        Ok(Some(cnt)) => {
            assert!(cnt > 0, "expected unmaker counter > 0");
        }
        Ok(None) => panic!("expected close() to return Some(counter) when final owner"),
        Err(e) => panic!("close() failed: {}", e),
    }
}
