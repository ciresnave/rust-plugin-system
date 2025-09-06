use plugin_interface::{PluginManager, PluginTrait, WatchOptions};
use std::error::Error;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new("../plugins_out");
    let mut mgr = PluginManager::new();

    let opts = WatchOptions {
        auto_load: true,
        auto_unload: true,
        emit_proxies: false,
        ..Default::default()
    };

    println!("Starting conservative background watcher for {:?}", dir);
    // start background watcher using a cloned options value created inline
    let (rx, stop_tx, _join) = mgr.start_watch_background(
        dir.to_path_buf(),
        WatchOptions {
            auto_load: true,
            auto_unload: true,
            emit_proxies: false,
            ..Default::default()
        },
    );

    println!("Processing notifications on manager thread (ctrl-c to quit)");
    mgr.process_watch_notifications_blocking(
        dir,
        rx,
        PluginTrait::Greeter,
        opts,
        |note| match note {
            plugin_interface::ManagerNotification::Event(ev) => {
                println!("manager event: {:?}", ev);
                true
            }
            plugin_interface::ManagerNotification::Unloaded { path, counter } => {
                println!("unloaded {:?} -> {:?}", path, counter);
                true
            }
            plugin_interface::ManagerNotification::Error(e) => {
                eprintln!("watch error: {}", e);
                true
            }
        },
    );

    // stop background watcher
    let _ = stop_tx.send(());
    Ok(())
}
