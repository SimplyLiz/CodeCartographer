#!/usr/bin/env bash
# Install the CodeCartographer binary from GitHub Releases.
# Env: CODECARTOGRAPHER_VERSION (tag or "latest"), GH_TOKEN (for API calls)
set -euo pipefail

REPO="anthropics/codecartographer"   # update to real org/repo name before publishing
BIN_DIR="${HOME}/.local/bin"
mkdir -p "${BIN_DIR}"

# Resolve "latest" to the actual tag.
if [[ "${CODECARTOGRAPHER_VERSION:-latest}" == "latest" ]]; then
  TAG=$(curl -fsSL \
    -H "Authorization: Bearer ${GH_TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
  if [[ -z "${TAG}" ]]; then
    echo "::error::Could not resolve latest CodeCartographer release. Check REPO=${REPO} and token permissions."
    exit 1
  fi
else
  TAG="${CODECARTOGRAPHER_VERSION}"
fi

echo "Installing CodeCartographer ${TAG}"

# Detect platform.
OS=$(uname -s)
ARCH=$(uname -m)

case "${OS}/${ARCH}" in
  Linux/x86_64)   PLATFORM="x86_64-unknown-linux-gnu" ;;
  Darwin/arm64)   PLATFORM="aarch64-apple-darwin" ;;
  Darwin/x86_64)  PLATFORM="x86_64-apple-darwin" ;;
  *)
    echo "::error::Unsupported platform ${OS}/${ARCH}. Build from source with install.sh."
    exit 1
    ;;
esac

ARTIFACT="codecartographer-binary-codecartographer-${PLATFORM}"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARTIFACT}.tar.gz"

echo "Downloading ${URL}"
TMP=$(mktemp -d)
curl -fsSL \
  -H "Authorization: Bearer ${GH_TOKEN}" \
  -H "Accept: application/octet-stream" \
  -o "${TMP}/codecartographer.tar.gz" \
  "${URL}"

tar -xzf "${TMP}/codecartographer.tar.gz" -C "${TMP}"
BINARY=$(find "${TMP}" -name "codecartographer" -type f | head -1)
if [[ -z "${BINARY}" ]]; then
  echo "::error::codecartographer binary not found in ${ARTIFACT}.tar.gz"
  exit 1
fi

install -m 755 "${BINARY}" "${BIN_DIR}/codecartographer"
rm -rf "${TMP}"

# Make sure the bin dir is on PATH for subsequent steps.
echo "${BIN_DIR}" >> "${GITHUB_PATH}"

echo "CodeCartographer $(codecartographer --version) installed to ${BIN_DIR}/codecartographer"
