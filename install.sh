#!/usr/bin/env bash
set -euo pipefail
REPO="Endlad2/Infinity-Backrooms"
ROOT="${HOME}/.backrooms_infinity"
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Linux) ASSET="BackroomsInfinity-linux-x64.tar.gz" ;;
  Darwin) echo "macOS release packaging is not enabled yet." >&2; exit 2 ;;
  *) echo "Unsupported OS: $OS" >&2; exit 2 ;;
esac
if [[ "$ARCH" != "x86_64" && "$ARCH" != "amd64" ]]; then echo "Only x64 release assets are published by current CI." >&2; exit 2; fi
URL="https://api.github.com/repos/${REPO}/releases/latest"
DOWNLOAD="$(python3 - "$URL" "$ASSET" <<'PYSCRIPT'
import json,sys,urllib.request
with urllib.request.urlopen(sys.argv[1]) as r: data=json.load(r)
for a in data['assets']:
    if a['name']==sys.argv[2]: print(a['browser_download_url']); break
else: raise SystemExit('release asset not found')
PYSCRIPT
)"
mkdir -p "$ROOT"
TMP="$(mktemp -d)"
curl -fL "$DOWNLOAD" -o "$TMP/release.tar.gz"
tar -xzf "$TMP/release.tar.gz" -C "$ROOT"
chmod +x "$ROOT/bin/"* || true
rm -rf "$TMP"
echo "Installed to $ROOT"
echo "Run: $ROOT/bin/backrooms_infinity_launcher"
