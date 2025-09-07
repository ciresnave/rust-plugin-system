use plugin_host::PluginManager;
use std::path::PathBuf;

fn plugin_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(".."); // workspace root
    p.push("plugins");
    p.push(name);
    p.push("target");
    p.push("debug");
    // on Windows the cdylib has .dll, on Unix .so
    #[cfg(target_os = "windows")]
    p.push(format!("{}.dll", name.replace('-', "_")));
    #[cfg(not(target_os = "windows"))]
    p.push(format!("lib{}.so", name.replace('-', "_")));
    p
}

fn plugin_source_dir(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("plugins");
    p.push(name);
    p
}

fn build_plugin(name: &str) {
    let src = plugin_source_dir(name);
    let manifest = src.join("Cargo.toml");
    let target_dir = src.join("target");
    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target_dir)
        .status()
        .expect("failed to spawn cargo build");
    assert!(status.success(), "cargo build failed for plugin {}", name);
}

#[test]
fn test_multi_registration_aggregation() {
    // Build the plugin crate before loading (we call cargo to ensure artifact exists)
    build_plugin("plugin-multi");
    let plugin_dir = plugin_path("plugin-multi");
    // plugin-host cargo test runs from plugin-host crate dir; the plugin artifacts are expected
    // under workspace/plugins/<plugin>/target/debug

    let mut mgr = PluginManager::new();
    let idx = mgr.load_plugin(plugin_dir).expect("load");
    // call greet on both registrations
    mgr.call_greet(idx, "test").expect("greet");
    // unloading should succeed
    mgr.unload_plugin(idx).expect("unload");
}

#[test]
fn test_fallback_to_single_registration() {
    // For this test we will use plugin-a which registers a single Greeter via plugin_register_Greeter_v1
    build_plugin("plugin-a");
    let plugin_dir = plugin_path("plugin-a");
    let mut mgr = PluginManager::new();
    let idx = mgr.load_plugin(plugin_dir).expect("load");
    mgr.call_greet(idx, "fallback").expect("greet");
    mgr.unload_plugin(idx).expect("unload");
}
