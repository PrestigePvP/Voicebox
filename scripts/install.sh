#!/usr/bin/env bash
# Build VoiceBox with the local signing identity and install it to /Applications.
#
# Requires src-tauri/tauri.local.conf.json (gitignored) naming the machine's
# code-signing identity — copy tauri.local.conf.example.json to create it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCAL_CONF="$ROOT/src-tauri/tauri.local.conf.json"
APP_DEST="/Applications/VoiceBox.app"
APP_BUILT="$ROOT/src-tauri/target/release/bundle/macos/VoiceBox.app"

if [[ ! -f "$LOCAL_CONF" ]]; then
  echo "Missing src-tauri/tauri.local.conf.json (gitignored — it names a" >&2
  echo "certificate that only exists on this machine). Create it with:" >&2
  echo >&2
  echo "  cp src-tauri/tauri.local.conf{.example,}.json" >&2
  echo >&2
  echo "then set signingIdentity to your code-signing certificate's name." >&2
  exit 1
fi

IDENTITY="$(jq -r '.bundle.macOS.signingIdentity // empty' "$LOCAL_CONF")"
if [[ -z "$IDENTITY" ]]; then
  echo "No bundle.macOS.signingIdentity in $LOCAL_CONF" >&2
  exit 1
fi

# Without this check Tauri falls back to ad-hoc signing and produces a
# "successful" build whose Accessibility and Microphone grants die on the next
# rebuild — a login item with a silently dead hotkey.
#
# Deliberately not `find-identity -v`: -v lists only identities whose chain is
# trusted, and a self-signed root is untrusted (CSSMERR_TP_NOT_TRUSTED).
# codesign accepts it anyway, and trusting the root would mean trusting it to
# sign anything — a real change in security posture for no benefit here.
if ! security find-identity -p codesigning | grep -qF "\"$IDENTITY\""; then
  echo "Code-signing identity '$IDENTITY' not found in the keychain." >&2
  echo "Create it: Keychain Access > Certificate Assistant > Create a Certificate" >&2
  echo "  Name: $IDENTITY / Self Signed Root / Code Signing" >&2
  exit 1
fi

echo "==> Signing identity: $IDENTITY"

pkill -x voicebox 2>/dev/null && echo "==> Quit running VoiceBox" || true

echo "==> Building"
cd "$ROOT"
cargo tauri build --bundles app --config "$LOCAL_CONF"

# Replace rather than copy over: leftover files from the previous bundle
# invalidate the signature.
echo "==> Installing to $APP_DEST"
rm -rf "$APP_DEST"
cp -R "$APP_BUILT" "$APP_DEST"

echo "==> Verifying"
codesign -dvv "$APP_DEST" 2>&1 | grep -E '^(Identifier|Authority)='

echo "==> Done. Open $APP_DEST"
