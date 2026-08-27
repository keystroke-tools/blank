# Blank

Blank is a deliberately small, self-hosted deployment platform for static frontend applications. It uses normal Linux primitives—Git, Mise, SQLite, immutable directories, symlinks, and systemd—and will embed Chimney for site serving.

### LLM usage disclosure

Blank was developed with assistance from large language models. Blank does not send your source code, repository credentials, request telemetry, or other project data to an LLM at runtime. Build and detection suggestions are generated locally from repository files.

Blank is not a container platform, a general-purpose PaaS, or a CI system.

Unauthenticated visitors see a compact landing page at `/`, with introductory documentation available at `/docs`. Signed-in administrators continue directly to the site dashboard.

## Development

Requirements: Rust, Node, and pnpm. Mise can install the pinned development tools.

```sh
mise install
pnpm --dir frontend install
pnpm --dir frontend build
cargo run
```

Blank listens on `127.0.0.1:8080` by default. Copy `.env.example` to `.env` to change its data directory, listener, or cookie mode. Production deployments must set `BLANK_SECURE_COOKIES=true` behind HTTPS.

The embedded Chimney listener uses `127.0.0.1:8081` by default and is configured with `BLANK_CHIMNEY_BIND`. Set `BLANK_CHIMNEY_HTTPS_PORT` and `BLANK_CHIMNEY_ACME_EMAIL` to enable Chimney-managed HTTPS. If HTTPS is configured before any active release exists, Blank starts HTTP-only so first-run setup remains available; restart after activating the first site to initialize TLS.

HTTP site, domain, and TLS certificate changes reload live through Chimney's reload API. Listener host, port, or HTTP/TLS topology changes still require a Blank restart because they would require rebinding sockets.

Repository build commands execute as the Blank service user. Adding a repository to Blank therefore grants its build process code execution as that user.

## Deployments

Blank can deploy a site manually from its overview page. Each deployment fetches the configured branch, creates an isolated Git worktree, installs project tools through Mise, runs the editable install/build commands, copies the publish directory into an immutable release, and atomically switches the site's `current` symlink only after success.

Active deployment logs stream to the browser over resumable Server-Sent Events. Completed logs remain available from deployment history without polling log bodies.

Successful historical deployments can be rolled back without rebuilding. Blank atomically switches to the retained release while preserving the site's current Chimney configuration. It keeps five successful releases by default; configure this with `BLANK_RELEASE_RETENTION`.

In site settings, **Detect settings** recognizes common Node projects and package managers from `package.json`, version files, Mise files, and npm, pnpm, Yarn, or Bun lockfiles. The result is only a suggestion: review it and save the form before deploying. An explicitly saved **Mise tools** list takes precedence over generated tool selections.

## Repository access

Blank uses the installed Git CLI and keeps one bare cache per site under its data directory. Public repositories work directly. For a private SSH repository, generate an Ed25519 deploy key from the site's settings, add the displayed public key to the Git provider as a read-only deploy key, then fetch the repository from Blank.

Private keys never leave the server and are stored with mode `0600`. Blank disables interactive Git credential prompts and keeps SSH known-host state inside its own data directory.

## Production deployment

Blank is distributed as one binary with the dashboard assets embedded in it. The server needs Linux, Git, OpenSSH's `ssh-keygen`, and Mise at runtime. Build the frontend before the Rust release binary so the current frontend is embedded:

```sh
git clone https://github.com/keystroke-tools/blank.git
cd blank
mise install
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
cargo build --release --locked
```

Install the binary and create a dedicated, non-login service account. Install Mise at a system-wide path such as `/usr/local/bin/mise`; repository builds run as the `blank` user and must be able to execute it.

For a host with a supported package manager, the repository includes an installer that creates the service account, builds the frontend and binary, installs the systemd unit, and configures daily rsyslog files:

```sh
sudo ./scripts/setup.sh
```

The installer is interactive by default and confirms service paths plus bind addresses, TLS, ACME email, public URL, expected DNS addresses, secure cookies, and release retention before changing anything. Set `BLANK_REPO`, `BLANK_REF`, or `BLANK_SKIP_PACKAGES=1` to customise it. Use `BLANK_NONINTERACTIVE=1` and provide the corresponding environment variables for unattended setup. The installer copies a Mise binary found under a home directory to `/usr/local/bin/mise` so `ProtectHome=true` does not hide it from the service.

```sh
sudo useradd --system --home-dir /var/lib/blank --create-home --shell /usr/sbin/nologin blank
sudo install -o root -g root -m 0755 target/release/blank /usr/local/bin/blank
sudo install -d -o blank -g blank -m 0750 /var/lib/blank
sudo install -d -o syslog -g adm -m 0750 /var/log/blank
```

Create `/etc/blank.env`:

```ini
BLANK_BIND=127.0.0.1:8080
BLANK_CHIMNEY_BIND=127.0.0.1:8081
BLANK_DATA_DIR=/var/lib/blank
BLANK_MISE_BIN=/usr/local/bin/mise
BLANK_SECURE_COOKIES=true
BLANK_RELEASE_RETENTION=5
BLANK_EXPECTED_IPS=203.0.113.10
BLANK_PUBLIC_URL=https://blank.example.com
RUST_LOG=blank=info,actix_web=info
```

Protect this file if it later contains environment-specific secrets:

```sh
sudo chown root:blank /etc/blank.env
sudo chmod 0640 /etc/blank.env
```

`BLANK_EXPECTED_IPS` is a comma-separated list used by domain checks. Set it to the public addresses that should receive site traffic. Keep `BLANK_SECURE_COOKIES=true` when the dashboard is served over HTTPS.

Set `BLANK_PUBLIC_URL` to the externally accessible HTTPS origin of the Blank dashboard. The new-site page can then create a private GitHub App through GitHub's manifest flow. Install that App on selected repositories to browse public and private repositories, fetch them with short-lived installation tokens, and receive signed push events without configuring each repository webhook manually.

For providers or repositories that are not connected through the GitHub App, `BLANK_WEBHOOK_SECRET` still enables the manual GitHub push webhook at `/api/webhooks/github`. Pushes matching a site repository and branch are queued automatically when that site has auto-deploy enabled.

### systemd

Create `/etc/systemd/system/blank.service`:

```ini
[Unit]
Description=Blank deployment platform
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=blank
Group=blank
WorkingDirectory=/var/lib/blank
EnvironmentFile=/etc/blank.env
ExecStart=/usr/local/bin/blank
Restart=on-failure
RestartSec=5s
TimeoutStopSec=30s
SyslogIdentifier=blank
StandardOutput=journal
StandardError=journal

# Blank deliberately executes repository build commands as this user.
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/blank

[Install]
WantedBy=multi-user.target
```

Enable it and inspect the initial startup:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now blank
sudo systemctl status blank
sudo journalctl -u blank -f
```

Database migrations run automatically at startup. Back up `/var/lib/blank` before replacing the binary, then restart the service:

```sh
sudo systemctl stop blank
sudo cp -a /var/lib/blank /var/lib/blank.backup
sudo install -o root -g root -m 0755 target/release/blank /usr/local/bin/blank
sudo systemctl start blank
```

### Daily log files

systemd continues to send output to the journal, while rsyslog can additionally route Blank records into files named by the date on which each record arrives. Create `/etc/rsyslog.d/30-blank.conf`:

```text
template(
  name="BlankDailyFile"
  type="string"
  string="/var/log/blank/%timegenerated:::date-year%-%timegenerated:::date-month%-%timegenerated:::date-day%.log"
)

if ($programname == "blank") then {
  action(
    type="omfile"
    dynaFile="BlankDailyFile"
    createDirs="on"
    dirCreateMode="0750"
    fileCreateMode="0640"
  )
  stop
}
```

Validate and reload rsyslog:

```sh
sudo rsyslogd -N1
sudo systemctl restart rsyslog
sudo systemctl restart blank
sudo tail -f /var/log/blank/$(date +%F).log
```

To update an existing installation, run the updater as root. It downloads the latest release binary for the host architecture, unless `BLANK_BINARY_URL` points to a specific asset or `BLANK_SOURCE_BUILD=1` requests a source build, then atomically replaces the binary and restarts Blank:

```sh
sudo ./scripts/update.sh
# or: sudo BLANK_REF=v1.2.0 ./scripts/update.sh
```

The updater interactively lets you choose the latest release, a source build, or a custom binary URL. It preserves the existing environment file and prompts for newly introduced or empty runtime settings before confirming the update. Set `BLANK_NONINTERACTIVE=1` for unattended updates that leave runtime configuration untouched.

This produces files such as `/var/log/blank/2026-08-23.log` and rolls to a new path at midnight without restarting Blank. The configuration assumes rsyslog is already receiving systemd journal records, as it does on common Debian and Ubuntu installations. If no file appears, verify that rsyslog's `imjournal` input or journald-to-syslog forwarding is enabled before changing the Blank service.

Because the date is already part of each filename, use systemd-tmpfiles for retention rather than rotating a file that Blank or rsyslog still has open. Create `/etc/tmpfiles.d/blank-logs.conf` to remove logs older than 30 days:

```text
d /var/log/blank 0750 syslog adm 30d
```

Test the cleanup rule without deleting anything, then let the normal systemd timer enforce it:

```sh
sudo systemd-tmpfiles --clean --dry-run /etc/tmpfiles.d/blank-logs.conf
```

### Public routing and TLS

The dashboard/API and deployed sites intentionally use separate internal listeners:

- `127.0.0.1:8080` serves the Blank dashboard and API.
- `127.0.0.1:8081` serves deployed sites through Chimney, selected by the request hostname.

Put both behind a public reverse proxy. Route only the dashboard hostname, such as `blank.example.com`, to port `8080`; route hosted-site hostnames to port `8081`. The proxy should preserve the original `Host`, `X-Forwarded-For`, and `X-Forwarded-Proto` headers, support streaming responses for deployment logs, and terminate TLS. Do not expose port `8080` directly to the internet.

For example, the essential Nginx routing shape is:

```nginx
server {
    listen 443 ssl http2;
    server_name blank.example.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
    }
}

server {
    listen 443 ssl http2 default_server;
    server_name _;

    location / {
        proxy_pass http://127.0.0.1:8081;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Supply appropriate certificates and an HTTP-to-HTTPS redirect using your normal Nginx or ACME setup. A default TLS server can only serve hostnames covered by its configured certificates; use explicit site blocks, a wildcard certificate, or automated certificate provisioning as appropriate for your domains.
