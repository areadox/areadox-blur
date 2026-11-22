#!/usr/bin/env bash
# Build and package Areadox Blur for Linux or macOS in the OBS 33 plugin layout.
#
#   scripts/package.sh            -> dist/ (zip/tar.gz ready to ship)
#   scripts/package.sh --install  -> additionally install for the current user
#
# Linux layout  : ~/.local/share/obs-studio/plugins/areadox-blur/{areadox-blur.so,data/}
# macOS layout  : ~/Library/Application Support/obs-studio/plugins/areadox-blur.plugin
set -euo pipefail

NAME="areadox-blur"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
VERSION="$(grep -m1 '^version = "' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
INSTALL=0
[[ "${1:-}" == "--install" ]] && INSTALL=1

cd "$ROOT"
rm -rf "$DIST"
mkdir -p "$DIST"

case "$(uname -s)" in
Linux)
    cargo build --release
    PKG="$DIST/$NAME"
    mkdir -p "$PKG"
    cp "target/release/libareadox_blur.so" "$PKG/$NAME.so"
    cp -r data "$PKG/data"
    tar -C "$DIST" -czf "$DIST/$NAME-$VERSION-linux-x86_64.tar.gz" "$NAME"
    echo "Paket: $DIST/$NAME-$VERSION-linux-x86_64.tar.gz"
    if [[ $INSTALL == 1 ]]; then
        TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/obs-studio/plugins/$NAME"
        rm -rf "$TARGET"
        mkdir -p "$(dirname "$TARGET")"
        cp -r "$PKG" "$TARGET"
        echo "Installiert nach $TARGET"
    fi
    ;;
Darwin)
    # Universal binary (Apple Silicon + Intel) if both targets are installed.
    TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
    LIBS=()
    for t in "${TARGETS[@]}"; do
        if rustup target list --installed | grep -q "^$t$"; then
            cargo build --release --target "$t"
            LIBS+=("target/$t/release/libareadox_blur.dylib")
        fi
    done
    if [[ ${#LIBS[@]} -eq 0 ]]; then
        cargo build --release
        LIBS=("target/release/libareadox_blur.dylib")
    fi
    BUNDLE="$DIST/$NAME.plugin"
    mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
    lipo -create "${LIBS[@]}" -output "$BUNDLE/Contents/MacOS/$NAME"
    cp -r data/* "$BUNDLE/Contents/Resources/"
    cat >"$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$NAME</string>
    <key>CFBundleDisplayName</key><string>Areadox Blur</string>
    <key>CFBundleIdentifier</key><string>de.areadox.areadox-blur</string>
    <key>CFBundleExecutable</key><string>$NAME</string>
    <key>CFBundlePackageType</key><string>BNDL</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict>
</plist>
PLIST
    # ad-hoc signature so Gatekeeper accepts the bundle locally
    codesign --force --sign - "$BUNDLE" || true
    (cd "$DIST" && zip -qr "$NAME-$VERSION-macos-universal.zip" "$NAME.plugin")
    echo "Paket: $DIST/$NAME-$VERSION-macos-universal.zip"
    if [[ $INSTALL == 1 ]]; then
        TARGET="$HOME/Library/Application Support/obs-studio/plugins/$NAME.plugin"
        rm -rf "$TARGET"
        mkdir -p "$(dirname "$TARGET")"
        cp -R "$BUNDLE" "$TARGET"
        echo "Installiert nach $TARGET"
    fi
    ;;
*)
    echo "Unter Windows bitte scripts/package.ps1 verwenden." >&2
    exit 1
    ;;
esac
