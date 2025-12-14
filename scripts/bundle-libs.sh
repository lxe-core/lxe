#!/bin/bash
# LXE Library Bundler Script
# Extracts all shared library dependencies and bundles them with the binary
# This enables the runtime to work on any Linux distro regardless of glibc version

set -euo pipefail

BINARY_NAME="${1:-lxe-runtime}"
BUILD_DIR="${BUILD_DIR:-/build}"
OUTPUT_DIR="${OUTPUT_DIR:-$BUILD_DIR/bundled}"
LIBS_DIR="$OUTPUT_DIR/libs"

echo "🔧 LXE Library Bundler"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Step 1: Build the runtime
echo "📦 Building $BINARY_NAME..."
cd "$BUILD_DIR"
cargo build --release -p lxe-runtime

BINARY_PATH="$BUILD_DIR/target/release/$BINARY_NAME"
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "❌ Error: Binary not found at $BINARY_PATH"
    exit 1
fi

# Step 2: Create output directory
echo "📁 Creating bundle directory..."
rm -rf "$OUTPUT_DIR"
mkdir -p "$LIBS_DIR"

# Step 3: Copy the binary
cp "$BINARY_PATH" "$OUTPUT_DIR/$BINARY_NAME"

# Step 4: Extract library dependencies
echo "🔍 Finding library dependencies..."
LIBS=$(ldd "$BINARY_PATH" | grep "=>" | awk '{print $3}' | grep -v "^$" || true)

# List of libraries to bundle (skip virtual/kernel libs)
SKIP_LIBS="linux-vdso|ld-linux|libc.so|libm.so|libdl.so|libpthread.so|librt.so"

for lib in $LIBS; do
    lib_name=$(basename "$lib")
    
    # Skip system libraries that should not be bundled
    if echo "$lib_name" | grep -qE "$SKIP_LIBS"; then
        echo "  ⏭️  Skipping system lib: $lib_name"
        continue
    fi
    
    if [[ -f "$lib" ]]; then
        echo "  📚 Bundling: $lib_name"
        cp "$lib" "$LIBS_DIR/"
        
        # Also copy any versioned symlinks
        real_lib=$(readlink -f "$lib")
        if [[ "$real_lib" != "$lib" && -f "$real_lib" ]]; then
            cp "$real_lib" "$LIBS_DIR/"
        fi
    fi
done

# Step 5: Bundle GTK4 modules and resources
echo "🎨 Bundling GTK4 resources..."

# GTK4 modules path
GTK_MODULE_PATH="/usr/lib/x86_64-linux-gnu/gtk-4.0"
if [[ -d "$GTK_MODULE_PATH" ]]; then
    mkdir -p "$OUTPUT_DIR/gtk-4.0"
    cp -r "$GTK_MODULE_PATH"/* "$OUTPUT_DIR/gtk-4.0/" 2>/dev/null || true
fi

# GDK-Pixbuf loaders
GDK_PIXBUF_PATH="/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0"
if [[ -d "$GDK_PIXBUF_PATH" ]]; then
    mkdir -p "$OUTPUT_DIR/gdk-pixbuf-2.0"
    cp -r "$GDK_PIXBUF_PATH"/* "$OUTPUT_DIR/gdk-pixbuf-2.0/" 2>/dev/null || true
fi

# Step 6: Patch RPATH to look for bundled libs first
echo "🔧 Patching binary RPATH..."
patchelf --set-rpath '$ORIGIN/libs' "$OUTPUT_DIR/$BINARY_NAME"

# Step 7: Create wrapper script
echo "📝 Creating wrapper script..."
cat > "$OUTPUT_DIR/run-$BINARY_NAME.sh" << 'EOF'
#!/bin/bash
# LXE Runtime Wrapper - Sets up bundled library environment
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export LD_LIBRARY_PATH="$SCRIPT_DIR/libs:${LD_LIBRARY_PATH:-}"
export GTK_PATH="$SCRIPT_DIR/gtk-4.0"
export GDK_PIXBUF_MODULE_FILE="$SCRIPT_DIR/gdk-pixbuf-2.0/2.10.0/loaders.cache"

exec "$SCRIPT_DIR/lxe-runtime" "$@"
EOF
chmod +x "$OUTPUT_DIR/run-$BINARY_NAME.sh"

# Step 8: Calculate bundle size
BUNDLE_SIZE=$(du -sh "$OUTPUT_DIR" | cut -f1)
LIB_COUNT=$(find "$LIBS_DIR" -name "*.so*" | wc -l)

echo ""
echo "✅ Bundle complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "   📦 Output: $OUTPUT_DIR"
echo "   📚 Libraries bundled: $LIB_COUNT"
echo "   💾 Total size: $BUNDLE_SIZE"
echo ""
echo "To use: ./run-$BINARY_NAME.sh [args]"
