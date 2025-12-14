# LXE (Linux Executable Environment)

> **Universal Package Format for Linux**
>
> Bridge the gap between portable apps and native installers.

LXE combines the portability of AppImage with the user experience of a native installer. It produces a single `.lxe` binary that, when run, launches a **GTK4 / Libadwaita** installation wizard to correctly install the application to the user's system (`~/.local/share` or `/usr/share`).

![Build Status](https://img.shields.io/github/actions/workflow/status/lxe-core/lxe/release.yml?style=flat-square)
![License](https://img.shields.io/crates/l/lxe?style=flat-square)
![Version](https://img.shields.io/crates/v/lxe?style=flat-square)

---

## ⚡ Features

*   **Single-File Distribution**: Ship one atomic binary (`myapp.lxe`).
*   **Native Installer Wizard**: Provides a familiar, polished setup experience using GTK4.
*   **System Integration**: Automatically handles `.desktop` files, icons, mime-types, and symlinks.
*   **Integrated Uninstaller**: Right-click "Uninstall" support out of the box.
*   **Zero Dependencies**: The packer (`lxe-cli`) is a static binary that runs on any CI/CD (GitHub Actions, GitLab) without setup.
*   **Auto-Detection**: `lxe init` scans `Cargo.toml`, `package.json`, or `setup.py` to auto-configure.
*   **Secure**: Built-in Ed25519 signing and verification.

---

## 🚀 Installation

### 1. Install CLI
Install the static binary to your system (requires `curl` and `bash`).

```bash
curl -fsSL https://raw.githubusercontent.com/lxe-core/lxe/main/install.sh | bash
```

### 2. Download Runtime
Fetch the runtime binary (required for building packages).

```bash
lxe runtime download
```

---

## 🛠️ Usage

### Initialize Project
Run this in your project root (works with Rust, Python, Node, Go, etc.).

```bash
lxe init
```

This generates an `lxe.toml` configuration file based on your project metadata.

### Build Package
After building your application (e.g., `cargo build --release` or `npm run build`), run:

```bash
lxe build
```

This acts as a packer, compressing your application (Zstd Level 19) and embedding it into the `.lxe` installer.

---

## 📦 Architecture

LXE consists of two main components:

1.  **lxe-cli** (The Packer): A static Rust binary used by developers to create packages.
2.  **lxe-runtime** (The Installer): A dynamic Rust binary (linked to GTK4) embedded inside every `.lxe` file. It handles the self-extraction and UI logic.

For a detailed comparison with AppImage, Flatpak, and Snap, see [docs/COMPARISON.md](docs/COMPARISON.md).

---

## 🔒 Security

Sign your packages using Ed25519 keys to prevent tampering.

```bash
# Generate keypair
lxe key generate

# Build signed package
lxe build --sign lxe-signing.key
```

---

## 📄 License
MIT or Apache-2.0
