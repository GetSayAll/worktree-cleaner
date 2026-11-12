#!/bin/sh
# Runs the development binary from inside a minimal .app bundle.
#
# Cargo calls a target runner as `runner <executable> <args>`. macOS assigns the
# Dock name when a GUI process registers with LaunchServices: from the bundle's
# Info.plist when the executable lives inside a .app, otherwise from the
# executable's file name. `tauri dev` builds `target/debug/worktree-cleaner` and
# would label the Dock with that name, so this wrapper assembles
# `Worktree Cleaner.app` beside it, links the binary in, and executes that. The
# bundled build already carries the same plist, so this only affects development.
set -eu

binary=$1
shift

# Test harnesses and any other target run directly; only the app gets a bundle.
if [ "$(basename -- "$binary")" != "worktree-cleaner" ]; then
  exec "$binary" "$@"
fi

binary_dir=$(CDPATH= cd -- "$(dirname -- "$binary")" && pwd)
binary=$binary_dir/$(basename -- "$binary")

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
config=$repo/src-tauri/tauri.conf.json
product_name=$(plutil -extract productName raw -o - "$config")
version=$(plutil -extract version raw -o - "$config")
identifier=$(plutil -extract identifier raw -o - "$config")

app=$binary_dir/$product_name.app
contents=$app/Contents
mkdir -p "$contents/MacOS" "$contents/Resources"
ln -sfn "$binary" "$contents/MacOS/$product_name"

plist=$contents/Info.plist
plutil -create xml1 "$plist"
plutil -insert CFBundleDevelopmentRegion -string English "$plist"
plutil -insert CFBundleDisplayName -string "$product_name" "$plist"
plutil -insert CFBundleExecutable -string "$product_name" "$plist"
plutil -insert CFBundleIdentifier -string "$identifier" "$plist"
plutil -insert CFBundleInfoDictionaryVersion -string 6.0 "$plist"
plutil -insert CFBundleName -string "$product_name" "$plist"
plutil -insert CFBundlePackageType -string APPL "$plist"
plutil -insert CFBundleShortVersionString -string "$version" "$plist"
plutil -insert CFBundleVersion -string "$version" "$plist"
plutil -insert NSHighResolutionCapable -bool YES "$plist"

icon=$repo/src-tauri/icons/icon.icns
if [ -f "$icon" ]; then
  ln -sfn "$icon" "$contents/Resources/icon.icns"
  plutil -insert CFBundleIconFile -string icon.icns "$plist"
fi

exec "$contents/MacOS/$product_name" "$@"
