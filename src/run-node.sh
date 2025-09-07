#!/usr/bin/env bash
set -Eeuo pipefail

GITHUB_TARBALL_URL="${GITHUB_TARBALL_URL:-https://github.com/augusthindenes/inf3200/releases/download/v0.1.0/webserver-x86_64-unknown-linux-musl.tar.gz}"

usage() {
    echo "Usage: $0 <node> <port>"
    echo "Set GITHUB_TARBALL_URL to the .tar.gz for the script to work env variable or in the script"
    exit 1
}

# --- Check that we have exactly 2 arguments ---
if [ $# -ne 2 ]; then
    usage
fi


NODE="$1"
PORT="$2"

# --- check that port is a number between 1 and 65535 ---
if ! [[ "$PORT" =~ ^[0-9]+$ ]] || (( PORT < 1 || PORT > 65535 )); then
    echo "Error: Port must be a number between 1 and 65535" >&2
    exit 2
fi

# --- workspace & cleanup ---
WORKDIR="$(mktemp -d)"
TARBALL="$WORKDIR/binary.tar.gz"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

echo "Entering ${NODE}"


# --- download tarball ---
echo "Downloading Webserver tarball from GitHub"
curl -fsS --no-progress-meter -L "$GITHUB_TARBALL_URL" -o "$TARBALL"
[[ -s "$TARBALL" ]] || { echo "Error: Download failed or empty file." >&2; exit 3; }

# --- ensure binary exists ---
mapfile -t entries < <(tar -tzf "$TARBALL")
BIN="${entries[0]}"
[[ -n "$BIN" ]] || { echo "Error: Could not find binary in tarball." >&2; exit 4; }

# -- extract and make binary executable ---
echo "Extracting Webserver binary"
tar -xzf "$TARBALL" -C "$WORKDIR"
BINPATH="$WORKDIR/$BIN"
chmod +x "$BINPATH"

echo "Starting webserver on port ${NODE}:${PORT} as background process"
nohup "$BINPATH" "$NODE" "$PORT" &> /dev/null &

echo "Exiting ${NODE}"
exit 0