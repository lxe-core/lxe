# LXE (Linux Executable Environment)

Linux distribution is fragmented. Shipping an app that works on Ubuntu, Fedora, Arch, and Debian usually involves building `.deb`, `.rpm`, and generic tarballs, or forcing users to install massive runtimes like Flatpak.

We built **LXE** to solve this. It's a new packaging format that combines the best parts of AppImage (portability) with the user experience of a native installer (MSI/DMG).

## What makes it different?

Unlike an AppImage, which runs from a temporary mount and often fails to integrate with the desktop (missing icons, no start menu entry), an `.lxe` file is a **Self-Extracting Installer**.

When a user opens your file:
1. It launches a native **GTK4 Setup Wizard**.
2. It installs your app to the correct XDG location (`~/.local/share` or `/usr/share`).
3. It creates standard menu entries, icons, and uninstallers.
4. It works on *any* distribution.

## For Developers

You don't need Docker, and you don't need to learn a complex spec file. LXE detects your project settings automatically.

### 1. Install LXE
```bash
curl -fsSL https://raw.githubusercontent.com/lxe-core/lxe/main/install.sh | bash
lxe runtime download
```

### 2. Initialize
Run this in your project root (Rust, Python, Node, Go, etc.):
```bash
lxe init
```
This reads your `Cargo.toml`, `package.json`, or `setup.py` and creates a simple `lxe.toml`.

### 3. Build using GitHub Actions (Recommended)
Because `lxe-cli` is a static binary, you can use it in any CI pipeline without installing dependencies.

```bash
lxe build
```

This produces a single binary: `myapp_1.0.0_x64.lxe`.

## Project Structure

*   `lxe-cli`: The packer tool. Zero dependencies, runs on any CI runner.
*   `lxe-runtime`: The installer stub. Uses system GTK4 libraries to draw the UI.
*   `lxe-common`: Shared logic for metadata and signing.

## License
MIT
