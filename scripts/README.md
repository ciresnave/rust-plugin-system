# Helper scripts

- build_plugins.ps1 - PowerShell helper to build all plugin crates and copy their built artifacts into `plugin-host/plugins_out`.
- build_plugins.sh - Bash helper for the same purpose.

Usage (PowerShell):

```powershell
cd <repo-root>/rust-plugin-system
scripts\build_plugins.ps1 -buildProfile debug
```

Usage (bash):

```bash
cd <repo-root>/rust-plugin-system
./scripts/build_plugins.sh
```

Notes:

- The scripts try a few common artifact names (dash vs underscore, lib prefix) and copy the first match.
- On Windows, prefer building plugins with the same target architecture as the host.
- You can use `plugin-host/examples/manager_watcher.rs` to watch `plugin-host/plugins_out` for newly copied plugin artifacts.
