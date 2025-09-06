# plugin-annotations

This crate implements the proc-macro utilities used to generate an FFI-safe plugin ABI.

Macros

- `#[plugin_interface]` — annotate a trait to emit a repr(C) vtable, registration structs, and
  loader helpers.
- `#[plugin_impl]` — annotate an `impl Trait for Type` to generate extern "C" wrappers,
  a `plugin_register_*` symbol, and a corresponding `plugin_unregister_*`.

## Notes about testing and in-process verification

- The proc-macros now emit a crate-local atomic counter and a versioned getter
  when you apply `#[plugin_aggregates(TraitName)]` at the crate root. The
  generated getter has the symbol name `plugin_unmaker_counter_<Trait>_v1` and
  returns the counter value as a `u64`. Generated unmaker functions
  increment the counter so host integration tests can call the getter and
  assert that unmakers ran without relying on filesystem side-effects.

Notes

- Generated wrappers use `std::panic::catch_unwind` so panics inside plugins do not unwind
  into the host.
- Registrations are allocated on the heap and freed by the unregister helpers.

## Documentation for the Plugin Annotations Crate

The `plugin-annotations` crate provides macros and attributes that developers can use to define plugin items in a concise and declarative manner. This crate is designed to simplify the process of creating plugins that conform to the specified plugin interface.

## Features

- **Declarative Annotations**: Use simple annotations to define plugin functionality without boilerplate code.
- **Automatic Code Generation**: The crate generates the necessary code for both the plugin and host sides during compilation, ensuring seamless integration.
- **Ease of Use**: Designed to be intuitive, allowing developers to focus on implementing their plugin logic rather than the underlying mechanics of the plugin system.

## Getting Started

To use the `plugin-annotations` crate in your project, add it as a dependency in your `Cargo.toml` file:

```toml
[dependencies]
plugin-annotations = { path = "../plugin-annotations" }
```

### Example Usage

This crate provides three macros used together to define a plugin ABI:

- `#[plugin_interface]` — place on a trait to generate an FFI-safe vtable and
  helper types.
- `#[plugin_impl(Trait)]` — place on an `impl Trait for Type` to generate the
  C wrappers plus `plugin_register_*` / `plugin_unregister_*` symbols for that
  implementation.
- `#[plugin_aggregates(Trait)]` — place at crate root to emit aggregated
  `plugin_register_all_<Trait>_v1` and `plugin_unregister_all_<Trait>_v1` helpers
  and the `plugin_unmaker_counter_<Trait>_v1` getter used by tests/hosts.

Minimal example (conceptual):

```rust
use plugin_annotations::plugin_interface;

#[plugin_interface]
pub trait Greeter {
  fn greet(&self, target: &str);
}

#[plugin_aggregates(Greeter)]
mod aggregates {}

#[plugin_impl(Greeter)]
impl Greeter for MyGreeter {
  fn greet(&self, t: &str) { println!("Hello, {}", t); }
}
```

The proc-macros take care of generating the external symbols your host will
look up at runtime.

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests to improve the functionality and documentation of this crate.

## License

This crate is licensed under the MIT License. See the `LICENSE` file for more details.
