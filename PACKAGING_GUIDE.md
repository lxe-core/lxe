# LXE Universal Packaging Guide

> **LXE is a Universally Compatible Format.** 
> It does NOT require AppImage, Flatpak, or DEB. It packages your **raw binaries** directly.

---

## 🚀 The Protocol (How to Package)

LXE works by taking your "build output directory" and wrapping it into a single, executed container.

### 1. Structure Your Build Output
After you build your app (e.g., `cargo build`, `npm run build`), you should have a folder like this:

```
staging/
├── myapp          # Your main executable
├── assets/        # Any data you need
└── icon.png       # App icon
```

### 2. Run the Packer
Point `lxe-pack` to that folder.

```bash
lxe-pack \
  --app-id com.example.MyApp \
  --exec "myapp" \
  --input ./staging \
  --output myapp.lxe
```

That's it. No special "recipe" files. No manifest. Just your files and the packer.

---

## ⚡ Examples

### 1. Rust / C++ / Go (Native)
**Best Case Scenario.** LXE produces tiny, ultra-fast packages.

```bash
# 1. Build
cargo build --release

# 2. Prepare staging
mkdir build_output
cp target/release/myapp build_output/
cp assets/icon.png build_output/

# 3. Package
lxe-pack --exec "myapp" --input build_output --output app.lxe ...
```

### 2. Python (Standalone)
Pack your script + a virtualenv.

```bash
# 1. Prepare
mkdir build_output
cp app.py build_output/
cp -r venv build_output/

# 2. Add Launcher (launch.sh)
echo '#!/bin/bash
exec ./venv/bin/python3 app.py "$@"' > build_output/launch.sh
chmod +x build_output/launch.sh

# 3. Package
lxe-pack --exec "launch.sh" --input build_output ...
```

### 3. Tauri / Electron (Web Tech)
Pack the final binary/bundle.

**Tauri:**
Tauri produces a single binary (and typically no sidecars unless configured).
```bash
cp target/release/my-tauri-app build_output/
lxe-pack --exec "my-tauri-app" --input build_output ...
```

**Electron:**
Pack the unzipped `dist` folder.
```bash
# electron-builder output
cp -r dist/linux-unpacked/* build_output/
lxe-pack --exec "myapp" --input build_output ...
```

---

## 🛡️ "Universal" Means...

1.  **Any Content**: Binaries, scripts, bytecode, assets.
2.  **Any Dependency Strategy**:
    *   **Static**: Compile everything into one binary (Rust/Go).
    *   **Bundled**: Include `.so` libs in your folder (like AppImage).
    *   **System**: Rely on host libraries (smaller size, but requires deps installed).

LXE doesn't force a philosophy. If you put it in the input folder, it runs.

---

## Real-World Proof: Velocity Bridge

We packaged "Velocity Bridge" (a complex GUI app) in two ways to demonstrate this:

1.  **Wrapper Method**: Wrapping the existing AppImage (Quick & Dirty).
    *   Result: ~29 MB
    *   Pros: Guaranteed to work universally immediately.
2.  **Native Method**: Packaging the raw Tauri binary directly.
    *   Result: **~6.5 MB** (4x smaller!)
    *   Pros: Showcases LXE's efficiency.
    *   *Note: Native packaging assumes system GTK/Webkit libs are present.*

LXE gives you the **choice** between portability (AppImage-style bundling) and efficiency (Native packaging).
