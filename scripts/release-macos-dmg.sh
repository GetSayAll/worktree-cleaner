#!/usr/bin/env bash

# Build, sign, notarize, staple and collect the macOS DMG.
#
# This is the only entry point a release uses. It reads credentials from the
# environment and never from the repository, so the same script works on a
# release Mac and on a CI runner:
#
#   APPLE_API_KEY_PATH       a decrypted .p8 on a protected temporary path
#   APPLE_API_KEY            the App Store Connect key id
#   APPLE_API_ISSUER         the App Store Connect issuer id
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: … (TEAMID)", or
#                            CODE_SIGN_IDENTITY from an isolated keychain
#
# Obtain them from the private credential repository; do not type a key id by
# hand and do not keep the .p8 anywhere but a temporary directory with mode 600.
#
# Usage:
#   scripts/release-macos-dmg.sh [--skip-gate] [--updater] [--allow-dirty]
#                                [--out <dir>]
#
#   --skip-gate    do not run `pnpm test` and `pnpm lint:rust` first
#   --updater      also produce the updater archive and its signature; this
#                  needs TAURI_SIGNING_PRIVATE_KEY and the manifest is written
#                  separately by scripts/release-assets.mjs
#   --allow-dirty  build from a tree with local changes (a local trial only)
#   --out          where the artifacts are collected (default: release)

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$REPO_ROOT"

SKIP_GATE=0
WITH_UPDATER=0
ALLOW_DIRTY=0
OUT_DIR="release"

while (( $# > 0 )); do
  case "$1" in
    --skip-gate)   SKIP_GATE=1 ;;
    --updater)     WITH_UPDATER=1 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    --out)         OUT_DIR="${2:?--out needs a directory}"; shift ;;
    -h|--help)     sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 64 ;;
  esac
  shift
done

die() { echo "release: $*" >&2; exit 1; }
step() { echo; echo "==> $*"; }

for command_name in cargo cmake codesign ditto git node pnpm shasum spctl xcrun; do
  command -v "$command_name" >/dev/null 2>&1 || die "required command is unavailable: $command_name"
done

# ---------------------------------------------------------------- credentials

# An isolated keychain run exports CODE_SIGN_IDENTITY; Tauri reads the other name.
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" && -n "${CODE_SIGN_IDENTITY:-}" ]]; then
  APPLE_SIGNING_IDENTITY="$CODE_SIGN_IDENTITY"
fi

[[ -n "${APPLE_SIGNING_IDENTITY:-}" ]] || die "APPLE_SIGNING_IDENTITY is not set"
[[ -n "${APPLE_API_KEY:-}" ]] || die "APPLE_API_KEY is not set"
[[ -n "${APPLE_API_ISSUER:-}" ]] || die "APPLE_API_ISSUER is not set"
[[ -n "${APPLE_API_KEY_PATH:-}" ]] || die "APPLE_API_KEY_PATH is not set"
[[ -f "$APPLE_API_KEY_PATH" ]] || die "APPLE_API_KEY_PATH does not exist"
[[ "$(stat -f '%Lp' "$APPLE_API_KEY_PATH")" == "600" ]] \
  || die "APPLE_API_KEY_PATH must have mode 600"

export APPLE_SIGNING_IDENTITY APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH

# --------------------------------------------------------------- preconditions

step "Preconditions"

if (( ALLOW_DIRTY == 0 )); then
  [[ -z "$(git status --porcelain)" ]] \
    || die "the working tree has local changes; commit them or pass --allow-dirty for a local trial"
fi

VERSION="$(node -p "require('./package.json').version")"
TAURI_VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"

[[ "$VERSION" == "$TAURI_VERSION" ]] \
  || die "package.json says $VERSION but src-tauri/tauri.conf.json says $TAURI_VERSION"
[[ "$VERSION" == "$CARGO_VERSION" ]] \
  || die "package.json says $VERSION but Cargo.toml says $CARGO_VERSION"
echo "version $VERSION"

if git rev-parse --verify --quiet "refs/tags/v$VERSION" >/dev/null; then
  echo "warning: tag v$VERSION already exists; this build reuses a released version" >&2
fi

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target list --installed | grep -qx "$target" \
    || die "rust target $target is missing; run: rustup target add $target"
done

# ----------------------------------------------------------------------- gate

if (( SKIP_GATE == 0 )); then
  step "Gate"
  pnpm test
  pnpm lint:rust
else
  step "Gate skipped"
fi

# ---------------------------------------------------------------------- build

CONFIG='{"bundle":{"createUpdaterArtifacts":false}}'
if (( WITH_UPDATER == 1 )); then
  [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] \
    || die "--updater needs TAURI_SIGNING_PRIVATE_KEY"
  CONFIG='{"bundle":{"createUpdaterArtifacts":true}}'
fi

step "Building the universal app and DMG"
pnpm tauri build --target universal-apple-darwin --config "$CONFIG"

BUNDLE_DIR="target/universal-apple-darwin/release/bundle"
APP="$BUNDLE_DIR/macos/Worktree Cleaner.app"
DMG="$(find "$BUNDLE_DIR/dmg" -maxdepth 1 -name '*_universal.dmg' | head -1)"

[[ -d "$APP" ]] || die "the app bundle was not produced at $APP"
[[ -n "$DMG" && -f "$DMG" ]] || die "the DMG was not produced under $BUNDLE_DIR/dmg"

# ------------------------------------------------------------- app validation

step "Validating the signed app"

codesign --verify --deep --strict --verbose=2 "$APP"
xcrun stapler validate "$APP" >/dev/null 2>&1 \
  || die "the app has no notarization ticket; the build did not notarize it"
spctl -a -vvv -t exec "$APP" 2>&1 | sed 's/^/    /'

lipo -archs "$APP/Contents/MacOS/worktree-cleaner" | grep -q 'x86_64' \
  || die "the app is not universal"
lipo -archs "$APP/Contents/MacOS/worktree-cleaner" | grep -q 'arm64' \
  || die "the app is not universal"

# ------------------------------------------------------------ dmg notarization

step "Notarizing the DMG"

xcrun notarytool submit "$DMG" \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
  --wait

xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
codesign --verify --verbose=2 "$DMG"
spctl -a -t open --context context:primary-signature -vv "$DMG" 2>&1 | sed 's/^/    /'

# -------------------------------------------------------------------- collect

step "Collecting into $OUT_DIR"

node scripts/release-assets.mjs collect \
  --platform darwin \
  --arch universal \
  --version "$VERSION" \
  --bundle "$BUNDLE_DIR" \
  --out "$OUT_DIR"

(
  cd "$OUT_DIR"
  shasum -a 256 ./*.dmg > SHA256SUMS
  if (( WITH_UPDATER == 1 )); then
    shasum -a 256 ./*.app.tar.gz >> SHA256SUMS
  fi
  chmod 0644 ./*
  cat SHA256SUMS
)

step "Done"
echo "Artifacts are in $REPO_ROOT/$OUT_DIR"
echo "Nothing was published: a tag, a push and a draft release are separate, deliberate steps."
