# LXE - Universal Linux Packages
> **The native, single-file packaging format for Linux.**

![LXE Banner](https://via.placeholder.com/1200x300.png?text=LXE+Universal+Packaging)

![Build Status](https://img.shields.io/github/actions/workflow/status/lxe-core/lxe/release.yml?style=for-the-badge)
![License](https://img.shields.io/crates/l/lxe?style=for-the-badge)
![Version](https://img.shields.io/crates/v/lxe?style=for-the-badge)

**Stop forcing your users to use the terminal.**

LXE creates self-extracting, single-file executables (`.lxe`) that install your application with a **native GTK4 wizard**. It combines the portability of AppImage with the user experience of a native installer.

---

## ⚡ The 30-Second Pitch

Why another format? Because Linux distribution is still painful.

*   **AppImage**: Great portability, but no installation, no menu entries, no icons.
*   **Flatpak/Snap**: Great ecosystem, but sandboxing headaches and huge runtimes.
*   **DEB/RPM**: Native UX, but dependency hell and distro-fragmentation.

**LXE is the middle ground:**
1.  **Single Binary**: You ship one file (`myapp.lxe`).
2.  **Native Installer**: User double-clicks → sees a beautiful setup wizard.
3.  **System Integration**: Automatically adds menu entries, icons, and symlinks.
4.  **Zero Dependencies**: The `lxe` CLI runs on any distro (static musl build).

---

## 🚀 Installation

Install the **LXE CLI** in seconds. No complex dependencies required.

```bash
# 1. Install the CLI tool
curl -fsSL https://raw.githubusercontent.com/lxe-core/lxe/main/install.sh | bash

# 2. Download the runtime (required for building)
lxe runtime download
```

You're ready to go. The CLI is tiny (~5MB) and works on any Linux distro.

---

## 🛠️ Developer Guide

Packaging with LXE is designed to be "zero-config" for most projects.

### 1. Initialize
Go to your project folder (Rust, Node, Python, etc.) and run:

```bash
lxe init
```

LXE intelligently detects your project details:
*   ✅ **Name & Version** (from `Cargo.toml`, `package.json`, `setup.py`)
*   ✅ **Executable** (scans `dist/`, `target/release/`)
*   ✅ **Icon** (finds `icon.png`, `logo.png`)

It generates a simple `lxe.toml` configuration file.

### 2. Build
Run your normal build process, then package it:

```bash
# Build your app (example)
cargo build --release  # or npm run build

# Package it
lxe build
```

**That's it.** You get a `myapp.lxe` binary ready to distribute.

---

## 📦 How It Works

An `.lxe` file is a **self-extracting executable** containing:

1.  **The Runtime**: A minimal bootstrap program.
2.  **Metadata**: JSON info about your app (ID, version, etc.).
3.  **Payload**: Your app files, highly compressed (Zstd level 19).
4.  **Signature**: Ed25519 cryptographic signature (optional).

When a user runs `myapp.lxe`:
1.  It verifies its own integrity.
2.  It launches a **GTK4 / Libadwaita** installation wizard.
3.  It installs your app to `~/.local/share/` (user) or `/usr/share/` (system).
4.  It creates a standard `.desktop` entry so your app appears in the system menu.

---

## 🆚 Comparison

| Feature | LXE | AppImage | Flatpak | Native (DEB/RPM) |
| :--- | :---: | :---: | :---: | :---: |
| **Distribution** | Single File | Single File | Repo / Ref | Repo / File |
| **Installation** | ✅ Native Wizard | ❌ None (Run-in-place) | ✅ Store / CLI | ✅ Package Mgr |
| **Menu Integration**| ✅ Auto | ❌ Manual | ✅ Auto | ✅ Auto |
| **Sandboxing** | Optional | Optional | 🔒 Forced | ❌ None |
| **Dependencies** | Bundled / System | Bundled | Runtime | System |
| **App Size** | ⭐ Smallest | Medium | Large | Small |

---

## 🔒 Security

LXE packages can be cryptographically signed.

```bash
# 1. Generate keys
lxe key generate

# 2. Build signed package
lxe build --sign lxe-signing.key

# 3. User verification
lxe verify myapp.lxe
```

This ensures the package hasn't been tampered with since you built it.

---

## ❤️ Contributing

LXE is open source. We love contributions!

*   **Core**: Rust (CLI and Runtime)
*   **UI**: GTK4 + Libadwaita

### Building from Source

```bash
git clone https://github.com/lxe-core/lxe
cd lxe

# Build CLI (no GTK required)
cargo build -p lxe-cli

# Build Runtime (requires GTK4 dev libs)
cargo build -p lxe-runtime
```

---

_License: MIT OR Apache-2.0_
