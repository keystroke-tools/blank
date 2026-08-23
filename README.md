# Blank

Blank is a deliberately small, self-hosted deployment platform for static frontend applications. It uses normal Linux primitives—Git, Mise, SQLite, immutable directories, symlinks, and systemd—and will embed Chimney for site serving.

Blank is not a container platform, a general-purpose PaaS, or a CI system.

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

HTTP site and domain changes reload live through Chimney's `ConfigHandle`. The current Chimney API constructs its TLS manager once, so certificate or TLS-domain changes still require a Blank restart. This should move to an upstream Chimney lifecycle API rather than a parallel Blank TLS implementation.

Repository build commands execute as the Blank service user. Adding a repository to Blank therefore grants its build process code execution as that user.

## Deployments

Blank can deploy a site manually from its overview page. Each deployment fetches the configured branch, creates an isolated Git worktree, installs project tools through Mise, runs the editable install/build commands, copies the publish directory into an immutable release, and atomically switches the site's `current` symlink only after success.

Active deployment logs stream to the browser over resumable Server-Sent Events. Completed logs remain available from deployment history without polling log bodies.

Successful historical deployments can be rolled back without rebuilding. Blank atomically switches to the retained release while preserving the site's current Chimney configuration. It keeps five successful releases by default; configure this with `BLANK_RELEASE_RETENTION`.

In site settings, **Detect settings** recognizes common Node projects and package managers from `package.json`, version files, Mise files, and npm, pnpm, Yarn, or Bun lockfiles. The result is only a suggestion: review it and save the form before deploying. Repository Mise configuration takes precedence over generated tool selections.

## Repository access

Blank uses the installed Git CLI and keeps one bare cache per site under its data directory. Public repositories work directly. For a private SSH repository, generate an Ed25519 deploy key from the site's settings, add the displayed public key to the Git provider as a read-only deploy key, then fetch the repository from Blank.

Private keys never leave the server and are stored with mode `0600`. Blank disables interactive Git credential prompts and keeps SSH known-host state inside its own data directory.
