#!/bin/sh
# Install the prebuilt `openrscad` CLI from GitHub Releases (macOS / Linux).
#
#   curl -fsSL https://raw.githubusercontent.com/matthova/openrscad/main/scripts/install.sh | sh
#
# Environment overrides:
#   OPENRSCAD_VERSION      release tag to install, e.g. v0.15.0 (default: latest)
#   OPENRSCAD_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   OPENRSCAD_REPO         GitHub owner/repo to download from (default: matthova/openrscad)
#
# The binaries are built by the `cli` job in .github/workflows/release.yml.

set -eu

REPO="${OPENRSCAD_REPO:-matthova/openrscad}"
VERSION="${OPENRSCAD_VERSION:-latest}"
INSTALL_DIR="${OPENRSCAD_INSTALL_DIR:-$HOME/.local/bin}"

err() {
  echo "openrscad-install: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-gnu ;;
  *) err "unsupported OS $(uname -s); build from source instead: https://github.com/$REPO#build-from-source" ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) err "unsupported architecture $(uname -m)" ;;
esac

# An x86_64 shell under Rosetta still reports x86_64; prefer the native binary.
if [ "$os" = apple-darwin ] && [ "$arch" = x86_64 ] &&
  [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
  arch=aarch64
fi

asset="openrscad-cli-$arch-$os.tar.gz"
if [ "$VERSION" = latest ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO "$2" "$1"; }
else
  err "need curl or wget"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "Downloading $asset ($VERSION)..."
fetch "$base/$asset" "$tmp/$asset" || err "download failed: $base/$asset"
fetch "$base/$asset.sha256" "$tmp/$asset.sha256" || err "download failed: $base/$asset.sha256"

expected="$(cut -d ' ' -f 1 <"$tmp/$asset.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | cut -d ' ' -f 1)"
else
  actual="$(shasum -a 256 "$tmp/$asset" | cut -d ' ' -f 1)"
fi
[ "$expected" = "$actual" ] || err "checksum mismatch for $asset"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
# Write beside the target and rename, so replacing a running binary is atomic.
cp "$tmp/openrscad" "$INSTALL_DIR/.openrscad.tmp"
chmod 755 "$INSTALL_DIR/.openrscad.tmp"
mv "$INSTALL_DIR/.openrscad.tmp" "$INSTALL_DIR/openrscad"

echo "Installed $("$INSTALL_DIR/openrscad" --version) to $INSTALL_DIR/openrscad"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Note: $INSTALL_DIR is not on your PATH. Add it, e.g.:
  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile" ;;
esac
