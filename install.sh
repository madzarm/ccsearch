#!/bin/sh
set -e

REPO="madzarm/ccsearch"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Darwin)
        case "${ARCH}" in
            arm64)  TARGET="aarch64-apple-darwin" ;;
            x86_64) TARGET="x86_64-apple-darwin" ;;
            *)      echo "Error: Unsupported architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    Linux)
        case "${ARCH}" in
            x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
            *)      echo "Error: Unsupported architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    *)
        echo "Error: Unsupported OS: ${OS}"
        exit 1
        ;;
esac

ASSET_NAME="ccsearch-${TARGET}"

# Get latest release tag
echo "Fetching latest release..."
TAG=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "${TAG}" ]; then
    echo "Error: Could not determine latest release"
    exit 1
fi

echo "Installing ccsearch ${TAG} for ${TARGET}..."

URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET_NAME}.tar.gz"
TMPDIR=$(mktemp -d)
trap 'rm -rf "${TMPDIR}"' EXIT

# Download and extract
curl -sL "${URL}" -o "${TMPDIR}/ccsearch.tar.gz"
tar xzf "${TMPDIR}/ccsearch.tar.gz" -C "${TMPDIR}"

# Install
if [ -w "${INSTALL_DIR}" ]; then
    mv "${TMPDIR}/ccsearch" "${INSTALL_DIR}/ccsearch"
else
    echo "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo mv "${TMPDIR}/ccsearch" "${INSTALL_DIR}/ccsearch"
fi

chmod +x "${INSTALL_DIR}/ccsearch"

echo "ccsearch ${TAG} installed to ${INSTALL_DIR}/ccsearch"
echo ""
echo "Run 'ccsearch --help' to get started."
