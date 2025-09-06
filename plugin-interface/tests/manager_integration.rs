use plugin_interface::{PluginManager, PluginTrait};
use std::path::PathBuf;

#[test]
fn manager_loads_plugins_and_unloads() {
    // Build path to plugin-multi debug artifact (assumes plugin was built by CI or earlier step)
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../plugins/plugin-multi/target/debug");
    #[cfg(target_os = "windows")]
    path.push("plugin_multi.dll");
    #[cfg(not(target_os = "windows"))]
    path.push("libplugin_multi.so");

    // Ensure plugin exists; if not, skip test.
    if !path.exists() {
        eprintln!("plugin artifact not found at {:?}; skipping", path);
        return;
    }

    let mut mgr = PluginManager::new();
    let handles = mgr
        .load_plugins(path.parent().unwrap(), PluginTrait::Greeter)
        .expect("failed to load plugins");
    assert!(!handles.is_empty());

    for h in handles {
        if let Some(g) = h.as_greeter() {
            g.greet("integration-test");
        }
        // call close and ensure it succeeds
        h.close().expect("close failed");
    }
}
