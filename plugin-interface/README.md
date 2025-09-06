# Rust Plugin System

This project implements a generic plugin interface using shared libraries (DLLs or SOs) in Rust. It allows developers to create plugins that can be dynamically loaded by a host application, enabling extensibility and modularity.

## Overview

The Rust Plugin System consists of three main components:

1. **Plugin Host**: The main application that loads and interacts with plugins.
2. **Plugin Interface**: Defines the traits and types that plugins must implement.
3. **Plugin Annotations**: Provides macros to simplify the creation of plugins.

## Getting Started

To get started with the Rust Plugin System, follow these steps:

1. Clone the repository:

```bash
git clone <repository-url>
cd rust-plugin-system
```

1. Build the host:

```bash
cd plugin-host
cargo build
```

1. Create plugins:

You can create your own plugins by implementing the required traits from the `plugin-interface` crate and using the macros provided by the `plugin-annotations` crate.

### Macro usage and placement

- Apply `#[plugin_aggregates(TraitName)]` once at the crate root of each plugin crate that will expose registrations for `TraitName`. This macro emits crate-level helpers `plugin_register_all_<Trait>_v1` and `plugin_unregister_all_<Trait>_v1`, and a versioned getter `plugin_unmaker_counter_<Trait>_v1` which returns an atomic counter value as `usize` for test/host inspection.
- Apply `#[plugin_impl(TraitName)]` to each `impl TraitName for YourType` to generate FFI-safe wrappers, a `plugin_register_<Trait>_<Type>_v1` maker function and a `plugin_unregister_<Trait>_<Type>_v1` unmaker function. Each impl is also submitted to an `inventory` collection so aggregated helpers can discover them.

### Ownership & safety

- Registrations returned by makers are heap-allocated by the plugin and are expected to be freed by the corresponding unmaker function. Aggregated registration arrays include a parallel `factories` pointer so the host can deterministically call the exact unmaker for each registration when needed.
- The host-side helper `unload_<trait>` (e.g., `unload_greeter`) is marked `unsafe` and requires the caller to ensure the `Library` and `RegistrationArray` invariants: the `RegistrationArray` must either be a host-owned array (in which case `factories` is null and the host will free allocations) or a plugin-owned array (in which case `factories` is non-null and the plugin owns allocations).
- Prefer the plugin-provided bulk unregister helper `plugin_unregister_all_<Trait>_v1` when available; otherwise the host will use `RegistrationFactory::unmaker` entries to free registrations deterministically.

## Helper: `get_unmaker_counter`

The `plugin-interface` crate provides a small helper `get_unmaker_counter(lib: &Library, trait_name: &str) -> Result<u64, String>` and a typed variant `get_unmaker_counter_for(lib: &Library, trait_id: PluginTrait) -> Result<u64, String>` which look up the generated `plugin_unmaker_counter_<Trait>_v1` symbol in a loaded `Library`, call it, and return the counter value. Use these helpers in host tests or tooling to assert that unmakers ran inside the plugin.

Example (host code):

```rust
use libloading::Library;
// after loading a plugin as `lib`
let count = plugin_interface::get_unmaker_counter(&lib, "Greeter")?;
assert!(count > 0u64);

// or using the typed API
let count2 = plugin_interface::get_unmaker_counter_for(&lib, plugin_interface::PluginTrait::Greeter)?;
assert!(count2 > 0u64);
```

## Load plugins

The host application will automatically discover and load plugins at runtime. Ensure that your plugins are compiled as dynamic libraries.

## Documentation

- **Plugin Host**: See `plugin-host/README.md` for details on how to use the host application.
- **Plugin Interface**: Refer to `plugin-interface/README.md` for information on creating plugins that conform to the interface.
- **Plugin Annotations**: Check `plugin-annotations/README.md` for guidance on using the provided macros to define plugins.

## Watcher and manager-owned pattern

The `plugin-interface` crate includes an optional watcher feature (Cargo feature `watch`) that helps hosts automatically discover new plugin dynamic libraries and optionally load/unload them. The watcher exposes two safe patterns:

- Blocking watcher: `PluginManager::watch_and_load_blocking(dir, trait_id, opts, callback)` — runs on the calling thread and can call `load_plugins` and return `PluginHandle` or typed proxies to the callback.
- Background conservative watcher: `PluginManager::start_watch_background(dir, opts)` — spawns a platform watcher thread and returns a Receiver of conservative `WatchNotification` values (path lists and unload notices). The caller (typically the same thread that owns the `PluginManager`) should then call `process_watch_notifications_blocking(dir, rx, trait_id, opts, callback)` to have the manager perform load/unload actions and emit `ManagerNotification` values.

### WatchOptions

When using the watcher APIs you can customize behavior via `WatchOptions`:

- `debounce_ms: u64` — debounce window (ms) used to coalesce rapid filesystem events.
- `recursive: bool` — whether to watch directories recursively.
- `auto_load: bool` — if true the manager will call `load_plugins` automatically when new files are discovered; otherwise callbacks receive empty handles/proxies and the discovered paths.
- `auto_unload: bool` — if true the manager will attempt to `unload_by_path` when files are removed or replaced.
- `emit_proxies: bool` — if true and the trait supports typed proxies (e.g., `Greeter`), the watcher will prefer sending typed proxies to the callback rather than raw `PluginHandle`s. Note: proxies are not Send/Sync and are only provided by the synchronous blocking watcher or manager-owned processing.

### Manager-owned watcher example

This pattern keeps the `PluginManager` as the single authority for load/unload operations and avoids sending non-Send plugin handles across threads.

```rust
use plugin_interface::{PluginManager, WatchOptions, PluginTrait};
use std::path::Path;

fn run_manager_watcher(dir: &Path) -> anyhow::Result<()> {
    let mut mgr = PluginManager::new();
    let opts = WatchOptions { auto_load: true, auto_unload: true, emit_proxies: false, ..Default::default() };

    // Start a conservative background watcher that only sends PathBuf lists.
    let (rx, stop_tx, _join) = mgr.start_watch_background(dir.to_path_buf(), opts.clone());

    // Process notifications on the manager-owning thread; this will call
    // load_plugins/unload_by_path and invoke the provided callback with
    // ManagerNotification values.
    mgr.process_watch_notifications_blocking(dir, rx, PluginTrait::Greeter, opts, |note| {
        match note {
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
        }
    });

    // To stop the background watcher thread, send a stop signal
    let _ = stop_tx.send(());
    Ok(())
}
```

### Notes

- Use `watch_and_load_blocking` if you want the watcher to run on the same thread as the manager and receive typed `PluginHandle` or proxies directly.
- Use the background watcher + `process_watch_notifications_blocking` if you prefer the watcher to run on a background thread and have the manager perform all loads/unloads on a single owning thread (recommended when working with non-Send plugin types).

## Contributing

Contributions are welcome! Please open an issue or submit a pull request for any enhancements or bug fixes.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.
