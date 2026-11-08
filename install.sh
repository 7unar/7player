#!/usr/bin/env bash
#
# Install the latest 7player release for Linux x86_64.
#
# Usage:
#   curl -fsSLO https://raw.githubusercontent.com/7unar/7player/main/install.sh
#   bash install.sh
#
# Optional environment variables:
#   VERSION=0.1.0     install a specific version instead of latest
#   PREFIX=/usr/local install root (default: $HOME/.local)
#   REPO=owner/repo   use a fork/mirror (default: 7unar/7player)

set -euo pipefail

REPO="${REPO:-7unar/7player}"
VERSION="${VERSION:-latest}"
PREFIX="${PREFIX:-$HOME/.local}"
TARGET="x86_64-unknown-linux-gnu"
ASSET="7player-${TARGET}.tar.gz"
CHECKSUM="${ASSET}.sha256"

fail() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}

case "$(uname -s)" in
    Linux) ;;
    *) fail "7player releases currently target Linux only (detected $(uname -s))." ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) ;;
    *)
        fail "no prebuilt binary for $(uname -m). Build from source: cargo install --git https://github.com/${REPO}.git --locked"
        ;;
esac

command -v curl >/dev/null 2>&1 || fail "curl is required."
command -v tar >/dev/null 2>&1 || fail "tar is required."
command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is required."

if [ -n "${BASE_URL:-}" ]; then
    : # explicit mirror/local directory, mainly for testing
elif [ "$VERSION" = "latest" ]; then
    # `releases/latest/download` can be flaky on some GitHub edges; resolve
    # the actual tag first so the asset URL is deterministic.
    api="https://api.github.com/repos/${REPO}/releases/latest"
    json="$(curl -fsSL --retry 3 "$api")" || fail "could not query GitHub API for the latest release; set VERSION explicitly to bypass this"
    tag="$(printf '%s' "$json" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$tag" ] || fail "could not determine the latest release tag; set VERSION explicitly"
    BASE_URL="https://github.com/${REPO}/releases/download/${tag}"
else
    BASE_URL="https://github.com/${REPO}/releases/download/v${VERSION#v}"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

printf 'Downloading 7player %s (%s)...\n' "$VERSION" "$TARGET"
curl -fsSL --retry 3 -o "${TMP_DIR}/${ASSET}" "${BASE_URL}/${ASSET}"
curl -fsSL --retry 3 -o "${TMP_DIR}/${CHECKSUM}" "${BASE_URL}/${CHECKSUM}"

(
    cd "$TMP_DIR"
    sha256sum -c "$CHECKSUM"
)

tar -xzf "${TMP_DIR}/${ASSET}" -C "$TMP_DIR"
[ -x "${TMP_DIR}/7player" ] || fail "archive did not contain the 7player binary."

install -Dm755 "${TMP_DIR}/7player" "${PREFIX}/bin/7player"

printf '\nInstalled 7player to %s/bin/7player\n' "$PREFIX"
"${PREFIX}/bin/7player" --version

case ":${PATH:-}:" in
    *":${PREFIX}/bin:"*) ;;
    *)
        printf '\nAdd it to your PATH:\n'
        printf '  export PATH="%s/bin:$PATH"\n' "$PREFIX"
        printf 'Then run: 7player\n'
        ;;
esac
