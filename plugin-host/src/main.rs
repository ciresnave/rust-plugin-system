// plugin-host/src/main.rs
// Simple example: start the conservative background watcher, then process
// notifications on the manager-owning thread so the manager performs
// load/unload actions. Adjust the plugin directory path as needed.

use plugin_interface::{PluginManager, PluginTrait, WatchOptions};
use std::path::PathBuf;

fn main() {
    // Directory to watch - change to your plugins output directory
    let watch_dir = PathBuf::from("./plugins_out");

    let mut mgr = PluginManager::new();

    let opts = WatchOptions {
        auto_load: true,
        auto_unload: true,
        emit_proxies: false,
        ..Default::default()
    };

    // Start background watcher (create a fresh options copy inline)
    let (rx, stop_tx, _jh) = mgr.start_watch_background(
        watch_dir.clone(),
        WatchOptions {
            auto_load: true,
            auto_unload: true,
            emit_proxies: false,
            ..Default::default()
        },
    );

    println!("Started background watcher for {:?}", watch_dir);

    // Process events on the manager thread. This will call load_plugins/unload_by_path
    // as needed and invoke the callback with ManagerNotification values.
    mgr.process_watch_notifications_blocking(&watch_dir, rx, PluginTrait::Greeter, opts, |note| {
        println!("manager notification: {:?}", note);
        true // keep processing
    });

    // To stop the watcher, send stop signal. (In this example we never reach here.)
    let _ = stop_tx.send(());
}
