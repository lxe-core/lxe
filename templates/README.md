# LXE Framework Templates

This directory contains ready-to-use `lxe.toml` templates for popular frameworks.

## Usage

Copy the template for your framework to your project root as `lxe.toml`, then customize it.

Or use the CLI:
```bash
lxe init --preset tauri
lxe init --preset electron
lxe init --preset python
```

## Available Templates

| Framework | File | Notes |
|-----------|------|-------|
| Tauri 2.x | `tauri.toml` | Handles sidecars correctly |
| Electron | `electron.toml` | For electron-builder output |
| Python | `python.toml` | PyInstaller one-file mode |
| Generic | `generic.toml` | Any compiled binary |

## Common Pitfalls Avoided

### Tauri Sidecars
- ⚠️ Sidecar must be named WITHOUT target triple (`server`, not `server-x86_64-unknown-linux-gnu`)
- ⚠️ Sidecar must be in SAME directory as main binary
- ✅ Template handles this automatically

### Icons
- ⚠️ Use your actual app icon, not a placeholder
- ✅ Template points to standard Tauri icon locations
