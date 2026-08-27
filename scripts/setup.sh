#!/usr/bin/env bash
set -Eeuo pipefail

# Install Blank from a source checkout and configure its system services.
# Override any of these variables when running the script:
#   BLANK_REPO, BLANK_REF, BLANK_SERVICE_USER, BLANK_DATA_DIR,
#   BLANK_INSTALL_DIR, BLANK_ENV_FILE, BLANK_SKIP_PACKAGES

BLANK_REPO="${BLANK_REPO:-https://github.com/keystroke-tools/blank.git}"
BLANK_REF="${BLANK_REF:-master}"
BLANK_SERVICE_USER="${BLANK_SERVICE_USER:-blank}"
BLANK_DATA_DIR="${BLANK_DATA_DIR:-/var/lib/blank}"
BLANK_INSTALL_DIR="${BLANK_INSTALL_DIR:-/usr/local/bin}"
BLANK_ENV_FILE="${BLANK_ENV_FILE:-/etc/blank.env}"
BLANK_SKIP_PACKAGES="${BLANK_SKIP_PACKAGES:-0}"
BLANK_SERVICE="${BLANK_SERVICE:-blank.service}"
BLANK_ROOT="$(mktemp -d /tmp/blank-setup.XXXXXX)"
trap 'rm -rf "$BLANK_ROOT"' EXIT

die() { printf 'blank setup: %s\n' "$*" >&2; exit 1; }
require_root() { [[ "$(id -u)" -eq 0 ]] || die 'run as root (for example: sudo scripts/setup.sh)'; }

if [[ -t 0 && "${BLANK_NONINTERACTIVE:-0}" != 1 ]]; then
  printf 'Blank interactive installer\n\n'
  read -r -p "Repository [$BLANK_REPO]: " value; [[ -z "$value" ]] || BLANK_REPO="$value"
  read -r -p "Git ref [$BLANK_REF]: " value; [[ -z "$value" ]] || BLANK_REF="$value"
  read -r -p "Service user [$BLANK_SERVICE_USER]: " value; [[ -z "$value" ]] || BLANK_SERVICE_USER="$value"
  read -r -p "Data directory [$BLANK_DATA_DIR]: " value; [[ -z "$value" ]] || BLANK_DATA_DIR="$value"
  read -r -p "Install directory [$BLANK_INSTALL_DIR]: " value; [[ -z "$value" ]] || BLANK_INSTALL_DIR="$value"
  read -r -p "Environment file [$BLANK_ENV_FILE]: " value; [[ -z "$value" ]] || BLANK_ENV_FILE="$value"
  printf '\nRepository: %s\nRef: %s\nService user: %s\nData: %s\nInstall: %s\n' "$BLANK_REPO" "$BLANK_REF" "$BLANK_SERVICE_USER" "$BLANK_DATA_DIR" "$BLANK_INSTALL_DIR"
  read -r -p 'Continue? [y/N] ' value
  [[ "$value" =~ ^[Yy]([Ee][Ss])?$ ]] || exit 0
fi

require_root

install_packages() {
  [[ "$BLANK_SKIP_PACKAGES" == 1 ]] && return
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y ca-certificates git openssh-client curl rsyslog build-essential pkg-config libsqlite3-dev
  elif command -v dnf >/dev/null 2>&1; then
    dnf install -y ca-certificates git openssh-clients curl rsyslog gcc gcc-c++ make pkgconf-pkg-config sqlite-devel
  elif command -v apk >/dev/null 2>&1; then
    apk add --no-cache ca-certificates git openssh-client curl rsyslog build-base pkgconf sqlite-dev
  else
    printf 'blank setup: no supported package manager found; install Git, OpenSSH, curl, rsyslog, Rust, Node, pnpm, and Mise manually.\n' >&2
  fi
}

install_packages
for command in git ssh-keygen curl; do command -v "$command" >/dev/null || die "missing required command: $command"; done
MISE_BIN="$(command -v mise || true)"
if [[ -z "$MISE_BIN" ]]; then
  for candidate in /home/*/.local/bin/mise /root/.local/bin/mise; do
    if [[ -x "$candidate" ]]; then MISE_BIN="$candidate"; break; fi
  done
fi
[[ -n "$MISE_BIN" ]] || die 'Mise is required to build Blank; install it and rerun (or set BLANK_SKIP_PACKAGES=1)'
if [[ "$MISE_BIN" == /home/* || "$MISE_BIN" == /root/* ]]; then
  install -o root -g root -m 0755 "$MISE_BIN" /usr/local/bin/mise
  MISE_BIN=/usr/local/bin/mise
fi

install -d -m 0755 "$BLANK_INSTALL_DIR"
install -d -o "$BLANK_SERVICE_USER" -g "$BLANK_SERVICE_USER" -m 0750 "$BLANK_DATA_DIR" 2>/dev/null || {
  if ! id "$BLANK_SERVICE_USER" >/dev/null 2>&1; then
    useradd --system --home-dir "$BLANK_DATA_DIR" --create-home --shell /usr/sbin/nologin "$BLANK_SERVICE_USER"
  fi
  install -d -o "$BLANK_SERVICE_USER" -g "$BLANK_SERVICE_USER" -m 0750 "$BLANK_DATA_DIR"
}

git clone --branch "$BLANK_REF" --depth 1 "$BLANK_REPO" "$BLANK_ROOT/source"
cd "$BLANK_ROOT/source"
mise install
mise exec -- pnpm --dir frontend install --frozen-lockfile
mise exec -- pnpm --dir frontend build
mise exec -- cargo build --release --locked
install -o root -g root -m 0755 target/release/blank "$BLANK_INSTALL_DIR/blank"

if [[ ! -f "$BLANK_ENV_FILE" ]]; then
  install -d -m 0755 "$(dirname "$BLANK_ENV_FILE")"
  cat >"$BLANK_ENV_FILE" <<EOF
BLANK_BIND=127.0.0.1:8080
BLANK_CHIMNEY_BIND=127.0.0.1:8081
BLANK_DATA_DIR=$BLANK_DATA_DIR
BLANK_MISE_BIN=$MISE_BIN
BLANK_SECURE_COOKIES=true
BLANK_RELEASE_RETENTION=5
BLANK_PUBLIC_URL=
EOF
  chown root:"$BLANK_SERVICE_USER" "$BLANK_ENV_FILE"
  chmod 0640 "$BLANK_ENV_FILE"
fi

install -d -o syslog -g adm -m 0750 /var/log/blank 2>/dev/null || install -d -m 0750 /var/log/blank
cat >/etc/systemd/system/blank.service <<EOF
[Unit]
Description=Blank deployment platform
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$BLANK_SERVICE_USER
Group=$BLANK_SERVICE_USER
WorkingDirectory=$BLANK_DATA_DIR
EnvironmentFile=$BLANK_ENV_FILE
ExecStart=$BLANK_INSTALL_DIR/blank
Restart=on-failure
RestartSec=5s
TimeoutStopSec=30s
SyslogIdentifier=blank
StandardOutput=journal
StandardError=journal
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$BLANK_DATA_DIR

[Install]
WantedBy=multi-user.target
EOF

cat >/etc/rsyslog.d/30-blank.conf <<'EOF'
template(name="BlankDailyFile" type="string" string="/var/log/blank/%timegenerated:::date-year%-%timegenerated:::date-month%-%timegenerated:::date-day%.log")
if ($programname == "blank") then {
  action(type="omfile" dynaFile="BlankDailyFile" createDirs="on" dirCreateMode="0750" fileCreateMode="0640")
  stop
}
EOF

systemctl daemon-reload
systemctl enable --now blank.service
systemctl restart rsyslog 2>/dev/null || true
printf 'Blank is installed. Dashboard: http://127.0.0.1:8080\n'
