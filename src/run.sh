#!/usr/bin/env bash
set -Eeuo pipefail

GITHUB_TARBALL_URL="${GITHUB_TARBALL_URL:-https://github.com/augusthindenes/inf3200/releases/latest/download/deploy-x86_64-unknown-linux-gnu.tar.gz}"

usage() {
    echo "Usage: $0 <nodes>"
    echo "Set GITHUB_TARBALL_URL to the .tar.gz for the script to work env variable or in the script"
    exit 1
}

# --- Check that nodes argument exists ---
if [ $# -ne 1 ]; then
    usage
fi

NODES="$1"

# --- check that nodes is a number between 1 and 100 ---
if ! [[ "$NODES" =~ ^[0-9]+$ ]] || (( NODES < 1 || NODES > 100 )); then
    echo "Error: Nodes must be a number between 1 and 100" >&2
    exit 2
fi
    

# --- Ensure we have the dependencies needed to fetch hostname ---
for cmd in curl tar hostname; do
    command -v "$cmd" >/dev/null || { echo "Missing dependency: $cmd" >&2; exit 3; }
done

# --- workspace & cleanup ---
WORKDIR="$(mktemp -d)"
TARBALL="$WORKDIR/binary.tar.gz"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT


# --- download tarball ---
echo "Downloading: $GITHUB_TARBALL_URL"
curl -fsSL "$GITHUB_TARBALL_URL" -o "$TARBALL"
[[ -s "$TARBALL" ]] || { echo "Error: Download failed or empty file." >&2; exit 4; }

# --- ensure binary exists ---
mapfile -t entries < <(tar -tzf "$TARBALL")
BIN="${entries[0]}"
[[ -n "$BIN" ]] || { echo "Error: Could not find binary in tarball." >&2; exit 5; }

# -- extract and make binary executable ---
tar -xzf "$TARBALL" -C "$WORKDIR"
BINPATH="$WORKDIR/$BIN"
chmod +x "$BINPATH"

# --- get hostname and run the binary ---
HOST="$(hostname -f 2>/dev/null || hostname)"

nohup "$BINPATH" "$HOST" "$NODES" &> /dev/null &

echo "Exiting node ${HOST}"
exit 0