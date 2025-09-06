# Plugin A

This is Plugin A, a dynamic library that implements the generic plugin interface defined in the `plugin-interface` crate. 

## Functionality

Plugin A provides specific functionalities that can be utilized by the host application. It is designed to be loaded dynamically at runtime, allowing for extensibility and modularity in the host application.

## Usage

To use Plugin A, ensure that it is properly configured in the host application. The host will load this plugin dynamically and call the exported functions as defined in the plugin interface.

## Installation

To include Plugin A in your project, add it as a dependency in your `Cargo.toml` file of the host application:

```toml
[dependencies]
plugin-a = { path = "../plugins/plugin-a" }
```

## Building

To build Plugin A, navigate to the `plugin-a` directory and run:

```bash
cargo build --release
```

This will compile the plugin into a dynamic library that can be loaded by the host application.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.