#!/bin/bash
# Build script to download and prepare CodeGraph bundle
# This script runs during the build process to embed CodeGraph

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GEN_DIR="$SCRIPT_DIR/gen"
CODEGRAPH_BUNDLE_DIR="$GEN_DIR/codegraph-bundle"
TARGET="win32-x64"
VERSION="0.9.4"

mkdir -p "$CODEGRAPH_BUNDLE_DIR"

# Check if codegraph is already bundled
if [ -f "$CODEGRAPH_BUNDLE_DIR/npm-shim.js" ]; then
    echo "CodeGraph bundle already exists, skipping download"
    exit 0
fi

echo "Downloading CodeGraph v${VERSION} for ${TARGET}..."

# Create temp directory for download
TMP_DIR=$(mktemp -d)
cd "$TMP_DIR"

# Download the release
ASSET_NAME="codegraph-${TARGET}.zip"
DOWNLOAD_URL="https://github.com/colbymchenry/codegraph/releases/download/v${VERSION}/${ASSET_NAME}"

curl -L -o "$ASSET_NAME" "$DOWNLOAD_URL"

# Extract
mkdir -p extracted
tar -xf "$ASSET_NAME" -C extracted --strip-components=1

# Move to bundle directory
rm -rf "$CODEGRAPH_BUNDLE_DIR"/*
mv extracted/* "$CODEGRAPH_BUNDLE_DIR/"

# Cleanup
cd "$SCRIPT_DIR"
rm -rf "$TMP_DIR"

echo "CodeGraph bundle prepared at $CODEGRAPH_BUNDLE_DIR"