# Plugin B

Plugin B is a dynamic library that implements the generic plugin interface defined in the `plugin-interface` crate. It is designed to be loaded by the host application and provides specific functionality that can be utilized through the plugin system.

## Features

- Implements the required traits from the plugin interface.
- Uses annotations from the `plugin-annotations` crate to automatically register itself with the host.
- Provides a set of functions that can be called by the host application.

## Installation

To use Plugin B, include it as a dependency in your host application's `Cargo.toml` file:

```toml
[dependencies]
plugin-b = { path = "../plugins/plugin-b" }
```

## Usage

After including Plugin B in your host application, you can load and interact with it using the plugin system. Refer to the documentation of the host application for details on how to load plugins and call their methods.

## Contributing

Contributions to Plugin B are welcome! Please feel free to submit issues or pull requests to improve its functionality or documentation.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.