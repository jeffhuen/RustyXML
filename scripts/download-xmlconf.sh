#!/bin/bash
# Download the W3C/OASIS XML Conformance Test Suite
#
# This script downloads the official XML Conformance Test Suite from the W3C.
# The test suite is ~50MB and contains 2000+ test cases from Sun, IBM, OASIS/NIST.
#
# Usage: ./scripts/download-xmlconf.sh
#
# After running, execute: mix test test/oasis_conformance_test.exs

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
XMLCONF_DIR="$PROJECT_ROOT/test/xmlconf"
TARBALL="xmlts20130923.tar.gz"
URL="https://www.w3.org/XML/Test/$TARBALL"

echo "W3C/OASIS XML Conformance Test Suite Downloader"
echo "================================================"
echo ""

# Check if already downloaded
if [ -d "$XMLCONF_DIR/xmlconf" ]; then
    echo "Test suite already exists at: $XMLCONF_DIR/xmlconf"
    echo "To re-download, remove the directory first:"
    echo "  rm -rf $XMLCONF_DIR/xmlconf"
    exit 0
fi

# Create directory
mkdir -p "$XMLCONF_DIR"
cd "$XMLCONF_DIR"

echo "Downloading from: $URL"
echo "Destination: $XMLCONF_DIR"
echo ""

# Download
if command -v curl &> /dev/null; then
    curl -LO "$URL"
elif command -v wget &> /dev/null; then
    wget "$URL"
else
    echo "Error: Neither curl nor wget found. Please install one of them."
    exit 1
fi

# Extract
echo ""
echo "Extracting..."
tar -xzf "$TARBALL"

# Cleanup
rm "$TARBALL"

echo ""
echo "Done! Test suite installed at: $XMLCONF_DIR/xmlconf"
echo ""
echo "Run conformance tests with:"
echo "  mix test test/oasis_conformance_test.exs"
echo ""
echo "Or include all tests (including not-wf):"
echo "  mix test test/oasis_conformance_test.exs --include skip"
