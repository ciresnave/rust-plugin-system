# What-Plug — Rust plugin ABI prototype

[![Clippy](https://github.com/ciresnave/rust-plugin-system/actions/workflows/clippy-fast.yml/badge.svg?branch=main)](https://github.com/ciresnave/rust-plugin-system/actions/workflows/clippy-fast.yml)

This workspace contains a small prototype demonstrating a Rust plugin ABI implemented
using a proc-macro (`plugin-annotations`) and a small shared interface crate
(`plugin-interface`). The host (`plugin-host`) dynamically loads plugin crates built
as `cdylib` (`plugins/plugin-a`, `plugins/plugin-b`) and calls into vtables exposed by
the plugins.

Key points

Run the example host (from the `plugin-host` folder):

```powershell
cd plugin-host
cargo run --example example_host
```

Run the integration test that loads and unloads `plugin-a`:

```powershell
cd plugin-host
cargo test -q
```

Next steps: improve the inventory aggregation, add more ABI-stable types, and
expand tests for safety/unload scenarios.

## Key points

- `plugin-annotations` provides two macros:
  - `#[plugin_interface]` placed on a trait emits a repr(C) vtable, registration types, and
    helper loader/unloader functions.
  - `#[plugin_impl]` placed on an `impl Trait for Type` generates extern "C" wrappers, a
    `plugin_register_{Trait}_v1` factory function, and `plugin_unregister_{Trait}_v1`.

  `plugin_register_all_{Trait}_v1` that returns a contiguous array of registrations (useful
  when a plugin crate exposes multiple registrations).

- The host uses `libloading` to open plugin shared libraries and prefers the aggregated
  `_all_` symbol when present; it falls back to the single registration symbol otherwise.
Next steps: improve the inventory aggregation, add more ABI-stable types, and expand tests for
safety/unload scenarios.

Host example (querying unmaker counter):

```rust
// after loading a plugin as `lib` (libloading::Library)
let count = plugin_interface::get_unmaker_counter(&lib, "Greeter")?;
assert!(count >= 0u64);

// typed variant
let count2 = plugin_interface::get_unmaker_counter_for(&lib, plugin_interface::PluginTrait::Greeter)?;
```

This project implements a generic plugin interface using shared libraries (DLLs or SOs) in Rust. It allows developers to create plugins that can be dynamically loaded by a host application, enabling extensibility and modularity in Rust applications.

## Project Structure

The project consists of several crates:

## Rust Plugin System

 This project implements a generic plugin interface using shared libraries (DLLs or SOs) in Rust. It allows developers to create plugins that can be dynamically loaded by a host application, enabling extensibility and modularity in Rust applications.

- **plugin-host**: The main application that loads and interacts with plugins.
- **plugin-interface**: Defines the traits and types that plugins must implement.
- **plugin-annotations**: Provides macros for annotating plugin items, generating necessary code for both the host and plugins.
- **plugins**: Contains specific implementations of plugins (e.g., Plugin A and Plugin B).

```bash
cd plugin-host
cargo run
```

## Creating Plugins

To create a new plugin, follow these steps:

1. Create a new directory under the `plugins` folder.
2. Implement the plugin functionality in `src/lib.rs`.
3. Use the macros provided by the `plugin-annotations` crate to register your plugin with the host.
4. Update the `Cargo.toml` file to include dependencies on `plugin-interface` and `plugin-annotations`.

## Documentation

Each crate has its own README file with detailed documentation on how to use it:

- **plugin-host/README.md**: Instructions for using the host application.
- **plugin-interface/README.md**: Guidelines for creating plugins that conform to the interface.
- **plugin-annotations/README.md**: Explanation of the macros available for defining plugins.
- **plugins/plugin-a/README.md**: Documentation for Plugin A.
- **plugins/plugin-b/README.md**: Documentation for Plugin B.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.
