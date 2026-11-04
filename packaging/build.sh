#!/usr/bin/env bash
# Packages target/release/ferrowl for distribution.
#   packaging/build.sh linux   → dist/Ferrowl-x86_64.AppImage, dist/ferrowl_<v>_amd64.deb (+ ferrowl_amd64.deb), dist/ferrowl-linux-x86_64.tar.gz
#   packaging/build.sh macos   → dist/Ferrowl-macos-arm64.dmg (Ferrowl.app inside), dist/ferrowl-macos-arm64.tar.gz
#   packaging/build.sh windows → dist/ferrowl-windows-x86_64.zip
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OS=${1:?linux|macos|windows}
ICONS=assets/icons/png
rm -rf dist && mkdir -p dist

case "$OS" in
linux)
  # Plain archive (binary + desktop entry + icon), as before.
  mkdir -p dist/tar && cp target/release/ferrowl README.md LICENSE packaging/linux/ferrowl.desktop dist/tar/
  cp "$ICONS/ferrowl-256.png" dist/tar/ferrowl.png
  tar -C dist/tar -czf dist/ferrowl-linux-x86_64.tar.gz . && rm -rf dist/tar

  # AppImage: one file, double-click to run, icon and menu entry included.
  APPDIR=dist/Ferrowl.AppDir
  mkdir -p "$APPDIR/usr/bin"
  cp target/release/ferrowl "$APPDIR/usr/bin/"
  cp packaging/linux/ferrowl.desktop "$APPDIR/"
  cp "$ICONS/ferrowl-256.png" "$APPDIR/ferrowl.png"
  for s in 16 32 48 64 128 256 512; do
    install -Dm644 "$ICONS/ferrowl-$s.png" "$APPDIR/usr/share/icons/hicolor/${s}x${s}/apps/ferrowl.png"
  done
  ln -s usr/bin/ferrowl "$APPDIR/AppRun"
  if [ ! -x "${APPIMAGETOOL:-}" ]; then
    APPIMAGETOOL=dist/appimagetool
    curl -fsSL -o "$APPIMAGETOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$APPIMAGETOOL"
  fi
  ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" --no-appstream "$APPDIR" dist/Ferrowl-x86_64.AppImage
  rm -rf "$APPDIR" dist/appimagetool

  # Debian/Ubuntu package (installs the menu entry and icons system-wide).
  cargo deb --no-build --no-strip --output dist/ >/dev/null
  # Same package under a fixed name, so the website can link
  # releases/latest/download/ferrowl_amd64.deb without knowing the version.
  cp dist/ferrowl_*_amd64.deb dist/ferrowl_amd64.deb
  ;;
macos)
  APP=dist/Ferrowl.app
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
  cp target/release/ferrowl "$APP/Contents/MacOS/"
  sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
  ICONSET=dist/ferrowl.iconset && mkdir -p "$ICONSET"
  for s in 16 32 128 256 512; do
    sips -z $s $s "$ICONS/ferrowl-1024.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) "$ICONS/ferrowl-1024.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/ferrowl.icns" && rm -rf "$ICONSET"
  # Ad-hoc signature: required for arm64 binaries after bundling (not notarized).
  codesign --force --deep --sign - "$APP"
  # Disk image with a shortcut to Applications for drag-and-drop install.
  STAGE=dist/dmg && mkdir -p "$STAGE" && cp -R "$APP" "$STAGE/" && ln -s /Applications "$STAGE/Applications"
  hdiutil create -volname Ferrowl -srcfolder "$STAGE" -ov -format UDZO dist/Ferrowl-macos-arm64.dmg >/dev/null
  rm -rf "$STAGE"
  # Plain archive of the app bundle.
  cp README.md LICENSE dist/
  tar -C dist -czf dist/ferrowl-macos-arm64.tar.gz Ferrowl.app README.md LICENSE
  rm -rf "$APP" dist/README.md dist/LICENSE
  ;;
windows)
  mkdir -p dist/zip && cp target/release/ferrowl.exe README.md LICENSE dist/zip/
  (cd dist/zip && 7z a ../ferrowl-windows-x86_64.zip . >/dev/null) && rm -rf dist/zip
  ;;
esac
ls -la dist
