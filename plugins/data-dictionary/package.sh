#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
VERSION="$(grep -m1 '<version>' "$ROOT/pom.xml" | sed -E 's/.*<version>([^<]+)<.*/\1/')"
PACKAGE_DIR="$ROOT/dist/dbx-data-dictionary-plugin-$VERSION"
ZIP_PATH="$ROOT/dist/dbx-data-dictionary-plugin-$VERSION.zip"
LATEST_ZIP_PATH="$ROOT/dist/dbx-data-dictionary-plugin-latest.zip"

cd "$ROOT"
mvn -q -DskipTests package

rm -rf "$PACKAGE_DIR" "$ZIP_PATH" "$LATEST_ZIP_PATH"
mkdir -p "$PACKAGE_DIR/bin" "$PACKAGE_DIR/lib"
cp "$ROOT/manifest.json" "$PACKAGE_DIR/manifest.json"
cp "$ROOT/bin/dbx-data-dictionary-plugin" "$PACKAGE_DIR/bin/dbx-data-dictionary-plugin"
cp "$ROOT/bin/dbx-data-dictionary-plugin.bat" "$PACKAGE_DIR/bin/dbx-data-dictionary-plugin.bat"
cp "$ROOT/target/dbx-data-dictionary-plugin-$VERSION-all.jar" "$PACKAGE_DIR/lib/dbx-data-dictionary-plugin.jar"
chmod +x "$PACKAGE_DIR/bin/dbx-data-dictionary-plugin"

(cd "$ROOT/dist" && zip -qr "dbx-data-dictionary-plugin-$VERSION.zip" "dbx-data-dictionary-plugin-$VERSION")
cp "$ZIP_PATH" "$LATEST_ZIP_PATH"
echo "$ZIP_PATH"
echo "$LATEST_ZIP_PATH"
