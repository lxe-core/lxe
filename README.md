# LXE

A packaging format for Linux that combines AppImage portability with proper installer experience.

## The Problem

Shipping an app on Linux means picking your poison: build multiple packages for different distros, or use AppImage (no menu entries, just a file sitting on the desktop). We wanted something that downloads as a single file but actually integrates with the system.

## What LXE Does

When you run a `.lxe` file, it launches a GTK4 setup wizard that:
- Extracts your app to the proper XDG location
- Creates desktop entries and menu shortcuts
- Registers an uninstaller
- Works on any distro with GTK4

The end user gets a familiar "install wizard" experience. You get one file that works everywhere.

## Getting Started

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/lxe-core/lxe/main/install.sh | bash
lxe runtime download
```

### Create a Package

```bash
# In your project root
lxe init
# Edit lxe.toml as needed
lxe build
```

That produces `yourapp.lxe`. Double-click to install.

### Example lxe.toml

```toml
[package]
name = "My App"
id = "com.example.myapp"
version = "1.0.0"
executable = "myapp"
icon = "icon.png"
description = "Does a thing"
categories = ["Utility"]

[build]
input = "./dist"
script = "cargo build --release && cp target/release/myapp dist/"
```

## Framework Presets

If you're using Tauri, Electron, or PyInstaller, there are templates:

```bash
lxe init --preset tauri
lxe init --preset electron
lxe init --preset python
```

These preconfigure the build script and directory structure.

## Package Signing

```bash
lxe key generate
```

Creates `lxe-signing.key`. Add it to your `lxe.toml`:

```toml
[build]
signing_key = "lxe-signing.key"
```

Users can verify with `lxe verify package.lxe`.

## CLI Reference

```
lxe init              Create lxe.toml (interactive)
lxe build             Build the package
lxe runtime download  Download the runtime stub
lxe runtime status    Check if runtime is installed
lxe key generate      Generate signing keypair
lxe verify <file>     Verify package signature
lxe uninstall <id>    Uninstall an app by ID
lxe self-update       Update lxe itself
```

## How It Works

A `.lxe` file is structured as:

```
[lxe-runtime binary] + [magic] + [metadata JSON] + [checksum] + [zstd payload] + [footer]
```

The runtime reads itself to find the footer, locates the metadata, and extracts the payload. No temp mounts, no FUSE required.

## Project Structure

```
lxe-cli/       CLI tool (static musl binary)
lxe-runtime/   Installer stub (links GTK4/libadwaita)
lxe-common/    Shared types and signing
```

## Using in CI

`lxe` is a static binary, so you can download it in any CI environment:

```yaml
- name: Install LXE
  run: |
    curl -fsSL https://raw.githubusercontent.com/lxe-core/lxe/main/install.sh | bash
    lxe runtime download
    lxe build
```

## License

MIT
