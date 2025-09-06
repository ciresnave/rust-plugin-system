use plugin_host::plugin_manager::PluginManager;
use std::path::PathBuf;

#[test]
fn load_call_unload_plugin() {
    // build path to plugin-a debug artifact
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("plugins");
    path.push("plugin-a");
    path.push("target");
    path.push("debug");
    #[cfg(target_os = "windows")]
    path.push("plugin_a.dll");
    #[cfg(target_os = "linux")]
    path.push("libplugin_a.so");
    #[cfg(target_os = "macos")]
    path.push("libplugin_a.dylib");

    let mut mgr = PluginManager::new();
    let idx = mgr.load_plugin(&path).expect("load failed");
    mgr.call_greet(idx, "test").expect("call greet failed");
    mgr.unload_plugin(idx).expect("unload failed");
}
