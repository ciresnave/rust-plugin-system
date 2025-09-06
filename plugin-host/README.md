# Rust Plugin System - Plugin Host

This directory contains the host application for the Rust Plugin System. The host is responsible for loading and managing plugins that conform to the defined plugin interface.

## Overview

The plugin host initializes the plugin system, dynamically loads plugins, and interacts with them through the plugin interface. It provides the necessary functionality to call exported functions from the plugins and manage their lifecycle.

## Getting Started

1. **Build the Host**: Use `cargo build` to compile the host application.
2. **Run the Host**: Execute `cargo run` to start the host and load the available plugins.
3. **Plugin Management**: The host will automatically discover and load plugins located in the designated plugins directory.

## Try it

Build the example plugins and run the host tests:

```powershell
cd plugin-host
cargo test
```

If you want to run the host example manually, build the plugin crates first:

```powershell
cd plugins\plugin-multi
cargo build
cd ..\..\plugin-host
cargo run --example example_host
```

Or run the manager-owned watcher example which listens for plugin files under `./plugins_out`:

```powershell
cd plugin-host
cargo run --example manager_watcher
```

## Inspecting unmaker counters

The `plugin-interface` crate provides a helper `get_unmaker_counter(lib: &Library, trait_name: &str) -> Result<u64, String>` you can call from the host to query the generated `plugin_unmaker_counter_<Trait>_v1` getter exported by a plugin. This is handy in tests to assert that unregister logic executed inside the plugin.

 Example (conceptual):

```rust
// after loading plugin as `lib`
let val = plugin_interface::get_unmaker_counter(&lib, "Greeter")?;
assert!(val > 0u64);
```

// Using the typed API avoids runtime string typos

```rust
let val2 = plugin_interface::get_unmaker_counter_for(&lib, plugin_interface::PluginTrait::Greeter)?;
assert!(val2 > 0u64);
```

## Plugin Interface

The host interacts with plugins through a defined interface. Ensure that your plugins implement the required traits and methods as specified in the `plugin-interface` crate.

## Plugin Lifecycle

The host manages the lifecycle of plugins, including loading, unloading, and calling their methods. Make sure to handle any necessary initialization and cleanup within your plugins.

## Contributing

If you wish to contribute to the plugin host or the overall plugin system, please follow the guidelines outlined in the main project README. Your contributions are welcome!

## License

This project is licensed under the MIT License. See the LICENSE file for more details.
