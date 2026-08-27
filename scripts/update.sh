#!/usr/bin/env bash
set -Eeuo pipefail

# Update an existing Blank installation. By default it downloads the latest
# GitHub release for the current Linux architecture. Set BLANK_BINARY_URL to
# use a specific asset, or BLANK_SOURCE_BUILD=1 to build from source.
BLANK_REPO="${BLANK_REPO:-https://github.com/keystroke-tools/blank.git}"
BLANK_REF="${BLANK_REF:-master}"
BLANK_RELEASE_REPO="${BLANK_RELEASE_REPO:-keystroke-tools/blank}"
BLANK_INSTALL_DIR="${BLANK_INSTALL_DIR:-/usr/local/bin}"
BLANK_SERVICE="${BLANK_SERVICE:-blank.service}"
BLANK_ENV_FILE="${BLANK_ENV_FILE:-/etc/blank.env}"
BLANK_BINARY_URL="${BLANK_BINARY_URL:-}"
BLANK_SOURCE_BUILD="${BLANK_SOURCE_BUILD:-0}"
WORK="$(mktemp -d /tmp/blank-update.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
declare -A PENDING_ENV=()

env_value() {
  [[ -f "$BLANK_ENV_FILE" ]] || return 0
  sed -n "s/^$1=//p" "$BLANK_ENV_FILE" | tail -n 1
}

effective_env_value() {
  local key="$1"
  if [[ -n "${PENDING_ENV[$key]-}" ]]; then printf '%s' "${PENDING_ENV[$key]}"; else env_value "$key"; fi
}

set_env_value() {
  local key="$1" value="$2" escaped
  [[ -n "$value" ]] || return 0
  escaped="${value//&/\\&}"
  if grep -q "^$key=" "$BLANK_ENV_FILE"; then
    sed -i "s|^$key=.*|$key=$escaped|" "$BLANK_ENV_FILE"
  else
    printf '%s=%s\n' "$key" "$value" >>"$BLANK_ENV_FILE"
  fi
}

prompt_missing_env() {
  local key="$1" label="$2" default="${3:-}" current value
  current="$(env_value "$key")"
  [[ -z "$current" ]] || return 0
  read -r -p "$label${default:+ [$default]}: " value
  value="${value:-$default}"
  [[ -z "$value" ]] || PENDING_ENV["$key"]="$value"
}

apply_pending_env() {
  local key
  for key in "${!PENDING_ENV[@]}"; do set_env_value "$key" "${PENDING_ENV[$key]}"; done
}

[[ "$(id -u)" -eq 0 ]] || { printf 'blank update: run as root\n' >&2; exit 1; }
if [[ -t 0 && "${BLANK_NONINTERACTIVE:-0}" != 1 ]]; then
  printf 'Blank interactive updater\n\n'
  read -r -p "Repository [$BLANK_REPO]: " value; [[ -z "$value" ]] || BLANK_REPO="$value"
  read -r -p "Git ref [$BLANK_REF]: " value; [[ -z "$value" ]] || BLANK_REF="$value"
  read -r -p "Pre-built binary URL [${BLANK_BINARY_URL:-latest release}]: " value; [[ -z "$value" ]] || BLANK_BINARY_URL="$value"
  if [[ -z "$BLANK_BINARY_URL" ]]; then
    read -r -p 'Build from source instead of using the latest release? [y/N] ' value
    [[ "$value" =~ ^[Yy]([Ee][Ss])?$ ]] && BLANK_SOURCE_BUILD=1
  fi
  read -r -p "Install directory [$BLANK_INSTALL_DIR]: " value; [[ -z "$value" ]] || BLANK_INSTALL_DIR="$value"
  read -r -p "Systemd service [$BLANK_SERVICE]: " value; [[ -z "$value" ]] || BLANK_SERVICE="$value"
  read -r -p "Environment file [$BLANK_ENV_FILE]: " value; [[ -z "$value" ]] || BLANK_ENV_FILE="$value"
  if [[ -f "$BLANK_ENV_FILE" ]]; then
    printf '\nChecking runtime configuration...\n'
    prompt_missing_env BLANK_PUBLIC_URL "Public dashboard URL"
    prompt_missing_env BLANK_EXPECTED_IPS "Expected public IPs, comma-separated"
    prompt_missing_env BLANK_BASE_DOMAIN "Default site base domain (for example pages.example.com)"
    prompt_missing_env BLANK_SECURE_COOKIES "Secure cookies" "true"
    prompt_missing_env BLANK_RELEASE_RETENTION "Successful releases to retain" "5"
    prompt_missing_env BLANK_CHIMNEY_HTTPS_PORT "Site HTTPS port (blank keeps TLS disabled)"
    if [[ -n "$(effective_env_value BLANK_CHIMNEY_HTTPS_PORT)" ]]; then
      prompt_missing_env BLANK_CHIMNEY_ACME_EMAIL "ACME email"
    fi
  else
    printf 'Environment file %s does not exist; runtime settings will not be changed.\n' "$BLANK_ENV_FILE" >&2
  fi
  if [[ -n "$BLANK_BINARY_URL" ]]; then
    printf '\nUpdate source: pre-built binary\n'
  elif [[ "$BLANK_SOURCE_BUILD" == 1 ]]; then
    printf '\nUpdate source: build %s (%s)\n' "$BLANK_REPO" "$BLANK_REF"
  else
    printf '\nUpdate source: latest GitHub release (%s)\n' "$BLANK_RELEASE_REPO"
  fi
  read -r -p 'Continue? [y/N] ' value
  [[ "$value" =~ ^[Yy]([Ee][Ss])?$ ]] || exit 0
fi
install -d "$BLANK_INSTALL_DIR"

architecture="$(uname -m)"
case "$architecture" in
  x86_64) target="x86_64-unknown-linux-gnu" ;;
  aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
  *) target="" ;;
esac

download_latest() {
  [[ -n "$target" ]] || return 1
  local release_json asset_url
  release_json="$(curl --fail --location --retry 3 --silent --show-error "https://api.github.com/repos/$BLANK_RELEASE_REPO/releases/latest")" || return 1
  asset_url="$(printf '%s' "$release_json" | sed -nE 's#.*"browser_download_url": "([^"]*blank-[^"]*-'"$target"'\.tar\.gz)".*#\1#p' | head -n1)"
  [[ -n "$asset_url" ]] || return 1
  curl --fail --location --retry 3 --output "$WORK/release.tar.gz" "$asset_url"
  tar -xzf "$WORK/release.tar.gz" -C "$WORK"
  [[ -f "$WORK/blank" ]] && cp "$WORK/blank" "$WORK/blank.new"
  [[ -f "$WORK/blank.new" ]]
}

if [[ -n "$BLANK_BINARY_URL" ]]; then
  curl --fail --location --retry 3 --output "$WORK/blank" "$BLANK_BINARY_URL"
elif [[ "$BLANK_SOURCE_BUILD" != 1 ]] && download_latest; then
  mv "$WORK/blank.new" "$WORK/blank"
else
  command -v mise >/dev/null || { printf 'blank update: Mise is required for source builds\n' >&2; exit 1; }
  MISE_BIN="$(command -v mise)"
  [[ "$MISE_BIN" != /home/* && "$MISE_BIN" != /root/* ]] || { printf 'blank update: Mise resolves inside a protected home directory; set BLANK_MISE_BIN to a system-wide binary\n' >&2; exit 1; }
  git clone --branch "$BLANK_REF" --depth 1 "$BLANK_REPO" "$WORK/source"
  cd "$WORK/source"
  mise install
  mise exec -- pnpm --dir frontend install --frozen-lockfile
  mise exec -- pnpm --dir frontend build
  mise exec -- cargo build --release --locked
  cp target/release/blank "$WORK/blank"
fi

apply_pending_env
chmod 0755 "$WORK/blank"
systemctl stop "$BLANK_SERVICE"
install -o root -g root -m 0755 "$WORK/blank" "$BLANK_INSTALL_DIR/.blank.new"
mv -f "$BLANK_INSTALL_DIR/.blank.new" "$BLANK_INSTALL_DIR/blank"
systemctl start "$BLANK_SERVICE"
systemctl --no-pager --full status "$BLANK_SERVICE" || true
printf 'Blank updated successfully.\n'
