# Blank

Build **Blank**, a small self-hosted frontend deployment platform.

Blank should feel like a deliberately tiny self-hosted Vercel/Netlify for static frontend applications, but it must **not** evolve into a general-purpose PaaS or CI/CD platform.

The application will be accessible at:

`https://blank.example.com`

The primary deployment target is a Linux VM or LXC running on Proxmox.

There must be:

* no Docker
* no containers managed by Blank
* no Kubernetes
* no Nixpacks/buildpacks
* no remote builders
* no general-purpose backend/service deployments

Blank should use normal Linux primitives:

* Rust
* Git
* Mise
* filesystem directories
* subprocesses
* SQLite
* systemd
* symlinks

The project should remain small, understandable, and easy to operate.

---

# 1. Core purpose

Blank deploys static frontend projects from Git repositories.

A deployment is fundamentally:

```text
Git repository
      ↓
checkout commit
      ↓
select project directory
      ↓
mise install
      ↓
optional dependency installation
      ↓
optional build
      ↓
select publish directory
      ↓
create immutable release
      ↓
atomically activate release
      ↓
embedded Chimney serves it
```

Blank owns:

* authentication
* Git integration
* repository management
* builds
* releases
* deployment logs
* domains/site configuration
* environment variables
* webhooks
* deployment history
* rollbacks
* the administrative UI

Chimney owns:

* HTTP static serving
* host matching
* TLS
* ACME
* redirects
* rewrites
* static-file behavior
* site routing

Do not duplicate Chimney functionality inside Blank.

---

# 2. Chimney integration

Use the reusable Rust Chimney library from:

`https://github.com/aosasona/chimney`

The repository currently contains:

```text
crates/
├── chimney-cli
└── chimney-core
```

Blank should depend on `chimney-core` and embed Chimney directly into the Blank process.

Do **not** require a standalone Chimney installation unless the library API ultimately makes a specific required feature impossible.

Do not shell out to the Chimney CLI when the library can perform the operation directly.

Blank and Chimney should conceptually have this relationship:

```text
chimney-core
      │
      ├── chimney-cli
      │
      └── Blank
```

Do not copy Chimney's implementation into Blank.

Where practical, use Chimney's public configuration types rather than defining duplicate serving configuration models.

Before implementing the server integration, inspect the current `chimney-core` API and design against the actual exported types and lifecycle.

Do not invent APIs based on assumptions.

---

# 3. Runtime architecture

Blank should preferably compile into one application binary.

Conceptually:

```text
┌────────────────────────────────────────────┐
│ Blank                                      │
│                                            │
│  Admin/API server                          │
│  Deployment engine                         │
│  Deployment workers                        │
│  Git integration                           │
│  Mise integration                          │
│  SQLite                                    │
│                                            │
│  Embedded Chimney                          │
│  ├── static serving                        │
│  ├── HTTP                                  │
│  ├── HTTPS                                 │
│  └── ACME                                  │
│                                            │
└────────────────────────────────────────────┘
```

Use Tokio for async execution.

Suggested backend stack:

```text
Rust
Axum
Tokio
SQLx
SQLite
Serde
tower-http
Argon2
```

Suggested frontend stack:

```text
React
TypeScript
Vite
Tailwind CSS

TanStack Router
TanStack Query
TanStack Table where useful
```

Use TanStack selectively where it solves a real problem.

In particular:

* **TanStack Router** should handle application routing
* **TanStack Query** should handle API data fetching, caching, invalidation, mutations, and loading/error state
* **TanStack Table** may be used for data-heavy views such as deployment history if it materially improves the implementation
* do not pull in additional TanStack packages simply because they exist

Use Tailwind CSS for the UI styling layer.

Do not introduce a second styling framework.

Avoid large component libraries unless a very small primitive library materially speeds up accessible UI implementation.

The frontend should compile to static assets and be embedded into or served by the Rust binary.

Avoid requiring Node on the production host for Blank itself.

---

# 4. Frontend architecture

Keep frontend architecture simple.

Suggested structure:

```text
frontend/src/
├── components/
├── features/
│   ├── auth/
│   ├── sites/
│   ├── deployments/
│   ├── configuration/
│   └── environment/
├── lib/
│   ├── api.ts
│   ├── query-client.ts
│   └── utilities.ts
├── routes/
├── styles/
├── main.tsx
└── router.tsx
```

Use:

* TanStack Router for route definitions
* TanStack Query for server state
* regular React state for local UI state
* Tailwind for styling

Do **not** add Redux, Zustand, MobX, or another global client-state framework unless a concrete need emerges.

Most Blank state should be either:

* server state managed by TanStack Query
* route state managed by TanStack Router
* local component state

Prefer URL state for things such as:

* selected deployment
* search terms
* deployment filters
* active settings sections where useful

Do not over-centralize UI state.

---

# 5. Frontend design principles

Blank's UI should feel:

* clean
* fast
* restrained
* developer-focused
* operational rather than marketing-heavy
* dense enough to be useful without becoming cluttered

Prefer:

* clear typography
* strong spacing
* restrained cards
* readable logs
* obvious status indicators
* good empty states
* good destructive-action confirmations

Avoid:

* giant gradients
* excessive animation
* glassmorphism
* huge dashboard charts
* unnecessary hero sections
* overly rounded everything
* generic SaaS dashboard clutter

Tailwind should be used to maintain a consistent design system.

Define reusable design tokens through Tailwind/CSS variables where useful, for example:

```text
background
surface
surface-muted
border
text
text-muted
primary
success
warning
danger
```

Do not scatter arbitrary visual values everywhere.

Support dark mode first if only one theme is implemented initially.

A light theme may be added later if straightforward.

---

# 6. Network listeners

Keep the admin interface separate from hosted sites.

Conceptually:

```text
Blank admin/API
localhost:8080

Hosted websites through embedded Chimney
:80
:443
```

The exact ports should be configurable.

In the target infrastructure, Caddy on the Proxmox host may forward or pass through traffic to Blank.

Blank should not attempt to manage Caddy or Proxmox.

Those are outside project scope.

---

# 7. Authentication

Support a small set of Blank administrators. The first administrator can add
more administrators after setup.

Do not build teams, organisations, invitations, billing, or RBAC. Every
administrator has the same full access.

On first startup:

```text
no admin exists
      ↓
/setup
      ↓
create administrator
      ↓
setup route disabled
```

Use:

* username or email
* password
* Argon2id password hashing
* secure server-side sessions
* HttpOnly cookies
* Secure cookies in production
* appropriate SameSite policy
* CSRF protection where relevant
* logout

Design it cleanly enough that additional auth mechanisms could eventually be added, but do not implement them now.

Frontend auth routing should use TanStack Router route guards/loaders where appropriate.

Do not duplicate authentication state throughout the component tree.

---

# 8. Site model

A Site represents a deployed frontend application.

It should contain approximately:

```text
Site
├── id
├── name
├── repository_url
├── repository_auth_id
├── branch
├── project_directory
├── install_command
├── build_command
├── publish_directory
├── build_enabled
├── auto_deploy
├── domains
├── chimney_config
├── chimney_config_origin
├── imported_chimney_hash
├── imported_chimney_commit
├── created_at
└── updated_at
```

Do not treat this exact schema as immutable.

Choose appropriate normalization where it materially improves the design.

---

# 9. Adding a site

The New Site flow should be a short wizard.

## Step 1 — Repository

Fields:

```text
Repository URL
Branch
Project directory
```

Defaults:

```text
Branch: main if discoverable
Project directory: .
```

Example monorepo:

```text
repo/
├── apps/
│   ├── api/
│   └── website/
├── packages/
└── pnpm-workspace.yaml
```

Blank must allow:

```text
Project directory: apps/website
```

The project directory and repository root are different concepts and must remain separate internally.

Use TanStack Query mutations for repository inspection and site creation.

The wizard should not become a giant multi-page flow.

Aim for the minimum number of steps needed to configure a site confidently.

---

# 10. Git repositories

Use the installed Git CLI rather than libgit2 unless there is a compelling technical reason otherwise.

Blank needs real Git behavior and interoperability.

Use commands such as:

```text
git clone
git fetch
git rev-parse
git log
git worktree
```

Do not repeatedly reclone repositories.

Maintain a repository cache.

A good layout would be:

```text
/var/lib/blank/repositories/<site-id>.git
```

Prefer a bare repository cache.

For each deployment:

```text
git fetch
      ↓
resolve target commit
      ↓
git worktree add deployment workspace
      ↓
build
      ↓
git worktree remove
```

This gives every deployment a clean checkout while avoiding repeated downloads.

---

# 11. Repository authentication

MVP should support:

## Public repositories

No credentials required.

## Private repositories

Support SSH deploy keys.

This should be the preferred authentication mechanism.

Blank may generate an Ed25519 deploy key pair for a site and display the public key so the user can add it to GitHub/GitLab/etc as a read-only deploy key.

Store private keys with strict filesystem permissions.

Optionally support HTTPS access tokens if easy to implement without significantly expanding scope.

Do not require GitHub OAuth for MVP.

---

# 12. GitHub-specific integration

Generic Git must work first.

After that, GitHub-specific integration may add:

* repository metadata
* commit author/message
* branch listing
* webhook handling
* push-to-deploy

Do not block Blank's initial implementation on a GitHub App.

Eventually a GitHub App may replace manual deploy-key/webhook setup, but that is not required in the first usable release.

---

# 13. Build settings

The build configuration should contain:

```text
Build enabled
Install command
Build command
Publish directory
```

Examples:

Static repository:

```text
Build enabled: false
Publish directory: .
```

Vite:

```text
Build enabled: true
Install: pnpm install --frozen-lockfile
Build: pnpm build
Publish: dist
```

Astro:

```text
Build enabled: true
Install: pnpm install --frozen-lockfile
Build: pnpm build
Publish: dist
```

Do not build a giant framework-detection system.

Blank may inspect common files such as:

```text
package.json
pnpm-lock.yaml
yarn.lock
package-lock.json
bun.lock
mise.toml
.mise.toml
```

and suggest sensible defaults.

Suggestions must remain editable.

Commands and paths are the actual source of truth.

---

# 14. Mise

Mise is the only supported runtime/tool installation mechanism for builds.

Do not install project Node/Python/Ruby/etc runtimes globally.

When a repository contains:

```text
mise.toml
```

or:

```text
.mise.toml
```

use it.

Typical build execution:

```text
cd <project-directory>

mise trust
mise install

mise exec -- <install-command>
mise exec -- <build-command>
```

Inspect Mise's current CLI behavior and implement this safely.

Repository Mise configuration should take precedence over Blank-provided tool configuration.

Eventually Blank may support selecting tools from the UI when no Mise file exists, for example:

```text
Node: 24
pnpm: 10
```

but this should not complicate the first implementation unnecessarily.

The simplest initial model may be:

* repository Mise file if present
* otherwise rely on commands using tools available through a Blank-generated temporary Mise environment

Do not introduce another runtime manager.

---

# 15. Chimney configuration import

This behavior is important.

When adding a site, after determining the project directory, inspect:

```text
<project-directory>/chimney.toml
```

If it exists:

1. read it
2. parse it using Chimney's configuration model where possible
3. validate it
4. import it into Blank's database
5. store the Git commit where it was imported
6. store a hash of the imported file

Once imported, the **database becomes the runtime source of truth**.

Do not automatically reread and apply the repository's `chimney.toml` during every deployment.

The repository file is an import source, not a permanent runtime binding.

---

# 16. Sites without chimney.toml

If no `chimney.toml` exists, Blank should create a basic site configuration through the UI.

The user should not be forced to add `chimney.toml` to their repository.

Blank should generate an appropriate Chimney configuration internally.

---

# 17. Chimney configuration editing

Blank must allow Chimney configuration to be edited directly from the UI.

Provide a structured editor for common values such as:

```text
Domains
Root
Index behavior
SPA fallback if supported
HTTPS/TLS-related options where relevant
Redirects where practical
```

Also provide an Advanced section with a TOML editor.

Both representations must modify the **same underlying configuration object**.

Conceptually:

```text
structured form
      ↓
Chimney config model
      ↑
raw TOML editor
```

Do not maintain independent form and TOML versions.

When raw TOML is edited:

1. parse it
2. validate it
3. show useful errors
4. only save valid configuration
5. update the structured form

If Chimney's Rust types are serializable and stable enough, use those internally.

Persisting a JSON representation in SQLite is acceptable.

TOML is primarily an import/export/edit representation.

Use TanStack Query mutations for save/import operations and invalidate only the relevant site configuration queries.

---

# 18. Upstream chimney.toml changes

After a repository `chimney.toml` has been imported, later repository changes must **not** silently overwrite the database configuration.

During a deployment, calculate the repository file hash if the file still exists.

If:

```text
current repo hash != imported hash
```

show:

```text
Repository chimney.toml changed since it was imported.
```

Provide options:

```text
View changes
Import repository version
Keep current configuration
```

Until the user explicitly imports the new version, the current database configuration remains active.

Never overwrite a dashboard edit because of a Git push.

---

# 19. Export chimney.toml

Provide:

```text
Export chimney.toml
```

This should serialize the current database configuration to a valid Chimney TOML file.

Do not automatically commit this file back to the repository.

Git write-back introduces credential and conflict handling that is outside MVP scope.

---

# 20. Filesystem layout

Do not deploy directly into a mutable `/var/www/site` directory.

Use immutable releases.

Suggested structure:

```text
/var/lib/blank/
├── blank.db
│
├── repositories/
│   └── <site-id>.git
│
├── builds/
│   └── <deployment-id>/
│
├── sites/
│   └── <site-id>/
│       ├── releases/
│       │   ├── <deployment-id-a>/
│       │   ├── <deployment-id-b>/
│       │   └── <deployment-id-c>/
│       │
│       └── current -> releases/<deployment-id-c>
│
├── keys/
│
└── state/
```

Exact names may change.

Important properties:

* build directories are temporary
* releases are immutable
* `current` identifies the active release
* activation is atomic
* failed deployments never modify the active release

---

# 21. Deployment lifecycle

A Deployment should use an explicit state machine.

Possible states:

```text
queued
fetching
checking_out
preparing
installing_tools
installing_dependencies
building
publishing
validating
activating
success
failed
cancelled
```

Static sites skip irrelevant states.

Example:

```text
queued
fetching
checking_out
publishing
validating
activating
success
```

Built site:

```text
queued
fetching
checking_out
installing_tools
installing_dependencies
building
publishing
validating
activating
success
```

Do not hardcode the UI around shell command names.

Model deployment steps structurally.

---

# 22. Deployment pipeline

Internally, organize deployment logic approximately as:

```text
FetchRepository
ResolveCommit
CreateWorktree
ResolveProjectDirectory
InstallMiseTools
InstallDependencies
BuildProject
ValidatePublishDirectory
CreateRelease
SnapshotConfiguration
ValidateSiteConfiguration
ActivateRelease
CleanupWorkspace
CleanupOldReleases
```

Implement this as reusable internal modules/steps rather than one enormous deployment function.

Do not overabstract into a generic CI engine.

These steps only exist to deploy frontend sites.

---

# 23. Build execution

Capture:

```text
stdout
stderr
exit status
timestamp
step
```

for subprocesses.

Environment:

* deployment-specific environment variables
* Mise environment
* minimal inherited environment
* required Git environment

Avoid leaking Blank's own secrets.

Do not run build subprocesses as root.

---

# 24. Deployment logs

Persist deployment logs.

The UI must show them live.

Use Server-Sent Events unless there is a strong reason not to.

WebSockets are unnecessary for simple one-way log streaming.

Example UI:

```text
Deploying 5e12f34

✓ Fetch repository             0.8s
✓ Checkout                     0.2s
✓ Install tools                2.4s
✓ Install dependencies         7.9s
● Build

> pnpm build
vite building...
✓ 341 modules transformed

○ Publish
○ Activate
```

Support viewing historical logs after deployment completion.

Use a dedicated log viewer component.

Important UX requirements:

* monospace output
* preserve whitespace
* follow latest output while deployment is active
* allow pausing auto-scroll
* allow manual scrolling
* differentiate deployment steps clearly
* do not render every log line as a React component if that causes performance problems
* keep very large logs usable

TanStack Query should manage deployment metadata.

SSE should feed live log/event state.

Do not repeatedly poll the full deployment log.

---

# 25. Deployment database model

Approximately:

```text
Deployment
├── id
├── site_id
├── commit_sha
├── commit_message
├── commit_author
├── status
├── triggered_by
├── created_at
├── started_at
├── finished_at
├── release_path
├── error_summary
└── config_snapshot
```

Also store build configuration relevant to the deployment so historical deployments remain understandable.

Do not store huge duplicated artifacts inside SQLite.

---

# 26. Config snapshots

Each successful deployment should record the site configuration active when that deployment occurred.

This includes at least:

* Chimney config snapshot
* build settings snapshot
* commit SHA
* relevant environment-variable version/reference

The active site configuration in the database remains independent from old releases.

---

# 27. Rollbacks

Keep a configurable number of successful releases.

Default:

```text
5
```

Rollback should generally switch the active release symlink.

Example:

```text
current -> release C

rollback to release A

current -> release A
```

Use an atomic filesystem operation.

A rollback must not require rebuilding.

By default, rollback only changes the **site files/release**.

It should continue using the site's **current Chimney configuration**.

Do not silently restore an old domain/configuration just because an old frontend version is activated.

The deployment page may separately offer:

```text
Restore configuration from this deployment
```

later.

That should be a distinct explicit action.

---

# 28. Atomic activation

Never write directly into the live release.

A deployment must work like:

```text
active release A

build release B
      ↓
validate release B
      ↓
validate config
      ↓
atomic switch A → B
```

If anything before activation fails:

```text
release A remains active
```

This applies to:

* dependency failures
* build failures
* malformed configuration
* invalid publish path
* publishing failures

Activation is the final mutating step.

---

# 29. Validation

Before activation validate at least:

* project directory exists
* project directory resolves within repository checkout
* publish directory exists
* publish directory resolves inside build workspace
* publish directory does not escape using `..`
* dangerous symlink/path traversal is prevented
* Chimney config parses
* Chimney config validates
* domains do not conflict with another Blank-managed site
* static root resolves to the intended release
* release was created successfully

An absent `index.html` may be a warning rather than a hard error because not all static layouts necessarily use one.

---

# 30. Environment variables

Frontend builds often require variables such as:

```text
VITE_API_URL
PUBLIC_API_URL
NEXT_PUBLIC_...
```

Support site-level environment variables.

UI example:

```text
Environment Variables

VITE_API_URL       ••••••••••
PUBLIC_NAME        Blank

[Add variable]
```

Sensitive values should:

* be encrypted at rest
* never be returned accidentally from API endpoints
* never appear in build logs due to Blank itself
* only be injected into build subprocesses where required

Use a locally generated application encryption key with appropriate filesystem permissions.

Do not build staging/development/preview environment profiles in MVP.

There is one production deployment target per site.

---

# 31. Automatic deployments

Support manual deployment first.

Then support Git push webhooks.

Per site:

```text
Automatic deployments: enabled
Production branch: main
```

For GitHub:

1. receive push webhook
2. validate webhook signature
3. confirm branch matches configured branch
4. identify target commit
5. ignore duplicate commit deployment if appropriate
6. enqueue deployment

Do not build a generic CI trigger engine.

---

# 32. Deployment queue

Only one deployment for a specific site may actively run at once.

Support a small global concurrency limit.

Default:

```text
2 builds
```

Other deployments wait in a queue.

SQLite is sufficient.

Do not introduce Redis.

Make queue recovery after process restart reliable.

A deployment left in a running state when Blank crashes should be marked interrupted/failed or safely recovered according to a clear startup policy.

---

# 33. Admin dashboard

Keep the dashboard simple.

Example:

```text
Blank

Sites                                  + New Site

● trace.example.com
  aosasona/trace
  main
  Deployed 4 minutes ago

● example-site.example.com
  aosasona/example
  main
  Deployed yesterday

○ docs.example.com
  aosasona/docs
  Last deployment failed
```

Avoid infrastructure-dashboard clutter.

No CPU graphs, container stats, cluster diagrams, etc.

Blank is a site deployment tool.

Use TanStack Query for dashboard data.

The dashboard should load quickly and should not fetch excessive historical data.

---

# 34. Site page

A site page should contain:

```text
example.com                     ● Online

Repository
github.com/foo/example

Branch
main

Project
apps/frontend

Latest deployment
5e12f34 · 4 minutes ago

[Deploy now]
```

Tabs or sections:

```text
Overview
Deployments
Configuration
Environment
Settings
```

Use nested TanStack Router routes where this improves deep linking.

For example:

```text
/sites/:siteId
/sites/:siteId/deployments
/sites/:siteId/configuration
/sites/:siteId/environment
/sites/:siteId/settings
```

Keep navigation shallow.

---

# 35. Deployments UI

List:

```text
✓ 5e12f34    4 min ago      18s
  Fix navigation

✓ 7ca91d1    Yesterday      22s
  Homepage updates

✕ 923fa33    Aug 20         14s
  Upgrade Vite
```

Deployment detail:

```text
Deployment 5e12f34

Status       Successful
Commit       5e12f34
Branch       main
Duration     18.2s

Fetch               ✓
Checkout            ✓
Tools               ✓
Dependencies        ✓
Build               ✓
Publish             ✓
Activate            ✓

Logs
...

[Rollback to this deployment]
```

TanStack Table may be used for deployment lists if it provides clear value for:

* pagination
* sorting
* filtering

Do not use it merely to render five rows.

---

# 36. API interaction

Create a small typed API client for the frontend.

Prefer generated or shared types where practical, but do not create an enormous API-generation system for this project.

TanStack Query query keys should be structured consistently.

Example conceptual keys:

```text
["sites"]
["sites", siteId]
["sites", siteId, "deployments"]
["deployments", deploymentId]
["sites", siteId, "configuration"]
```

Mutations should invalidate or update the minimum relevant cache entries.

Avoid global "invalidate everything" behavior.

Use optimistic updates only where they actually improve UX and are safe.

Do not use optimistic updates for deployment activation or other operations where backend confirmation matters.

---

# 37. Forms

Use normal React forms or a lightweight form solution if it materially improves validation.

Do not add a large form framework without reason.

Forms should:

* validate client-side for immediate feedback
* still rely on backend validation as authoritative
* display backend errors clearly
* preserve unsaved edits where reasonable

Important forms include:

* setup
* login
* new site
* build settings
* Chimney config
* environment variables
* site settings

Tailwind should provide consistent input, label, helper-text, error, and button styles.

---

# 38. Site configuration UI

Normal configuration view should expose the most useful Chimney fields as controls.

For example:

```text
Domains

example.com
www.example.com

Root
.

HTTPS
Enabled
```

Do not assume exact fields before inspecting Chimney's current configuration API.

Build the form around supported Chimney configuration.

Then provide:

```text
Advanced
Edit chimney.toml
```

The advanced TOML editor is especially important because Blank should not have to create bespoke UI for every Chimney capability immediately.

A lightweight code editor may be used for TOML if appropriate.

Do not pull in an enormous IDE framework unless necessary.

---

# 39. Domain configuration

A site can have multiple domains.

Domains must be globally unique among active Blank sites unless Chimney explicitly supports a legitimate overlapping configuration.

Display conflicts clearly.

Domain modifications should update the embedded Chimney serving configuration without requiring Blank to restart if Chimney's API supports runtime reconfiguration.

If the current Chimney API does not support safe runtime mutation:

* inspect how Chimney itself reloads sites
* use the least disruptive supported approach
* do not invent a second router/server
* do not restart the entire process unless absolutely necessary

---

# 40. Embedded Chimney lifecycle

At Blank startup:

```text
load DB
      ↓
load active sites
      ↓
construct Chimney site configuration
      ↓
start Chimney
```

When a site changes:

```text
DB update
      ↓
validate
      ↓
update embedded Chimney configuration
```

When a release activates:

```text
current release changes
      ↓
Chimney serves new root immediately
```

Investigate the actual public API available in `chimney-core`.

If Chimney needs library improvements to support clean dynamic configuration, prefer making those changes upstream in Chimney rather than implementing parallel server functionality inside Blank.

---

# 41. Health

Blank should expose internal health information.

At minimum:

```text
Blank
Running

Database
Healthy

Git
Available

Mise
Available

Chimney
Running

Active sites
N
```

Do not turn this into server monitoring software.

---

# 42. Host installation

Create a small installer for a fresh Linux VM/LXC.

Target Debian/Ubuntu first.

The installer should:

1. verify architecture/platform
2. create an unprivileged `blank` system user
3. create `/var/lib/blank`
4. set correct permissions
5. install/copy the Blank binary
6. ensure Git exists
7. install Mise if not available
8. install the Blank systemd unit
9. enable Blank
10. start Blank
11. run a health check

Do not install Docker.

Do not install Chimney separately because it is embedded.

Do not install project runtimes globally.

---

# 43. systemd

Blank should run under systemd.

Conceptually:

```ini
[Unit]
Description=Blank frontend deployment server
After=network-online.target
Wants=network-online.target

[Service]
User=blank
Group=blank
ExecStart=/usr/local/bin/blank
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
```

Add appropriate security hardening where it does not interfere with deployments.

For example consider:

* NoNewPrivileges where compatible
* restricted writable paths
* explicit environment/config file
* sensible file descriptor limits

Do not blindly enable hardening flags that break Mise, Git, or builds.

---

# 44. Privileges

Blank should not normally run as root.

Installation may require root.

Runtime should use an unprivileged user.

The deployment worker and hosted site server should run inside the same Blank process/user unless the Chimney library requires another arrangement.

The Blank user owns:

```text
/var/lib/blank
```

Blank may need capability handling or external port forwarding for ports below 1024.

Do not simply run the entire application as root to get ports 80/443.

Choose an appropriate Linux solution such as:

* systemd socket activation if suitable
* `CAP_NET_BIND_SERVICE`
* forwarding higher internal ports
* another simple secure mechanism

Document the choice.

---

# 45. HTTPS and the Blank admin domain

Blank's admin UI will ultimately be reachable at:

`https://blank.example.com`

Do not hardcode this hostname deeply into application logic.

Allow configuring the external/base URL.

Use it for:

* generated webhook callback URLs where applicable
* UI links
* secure-cookie behavior if necessary

Hosted user sites may have arbitrary domains.

Blank should delegate their actual serving/TLS behavior to Chimney.

---

# 46. Security

This application executes repository-controlled build commands, so treat deployments as trusted-code execution by design.

Do not pretend to provide strong sandboxing.

However enforce clear filesystem boundaries.

At minimum:

* reject project path traversal
* reject publish path traversal
* validate worktree paths
* prevent release-path escapes
* validate symlink behavior during publishing
* securely store SSH keys
* securely store environment variables
* validate webhook signatures
* use secure sessions
* avoid shell interpolation where possible
* spawn commands with argument arrays when practical
* never build commands by concatenating untrusted input into a shell unnecessarily

Install/build commands are intentionally user-configurable.

Make the risk explicit in documentation:

> Adding a repository to Blank grants its build process code execution as the Blank service user.

This is a trusted self-hosted tool, not a multi-tenant untrusted build service.

---

# 47. Logging

Use structured application logs.

Record events such as:

```text
site created
site updated
deployment queued
deployment started
deployment failed
deployment completed
release activated
rollback activated
webhook received
configuration imported
configuration changed
```

Do not put secrets into logs.

Build logs are separate from application logs.

---

# 48. Persistence

Use SQLite.

Use migrations.

Do not introduce Postgres as a requirement.

Blank should remain easy to back up:

```text
/var/lib/blank/
```

should contain almost all persistent state.

Document what must be backed up:

* SQLite database
* encrypted secret key
* repository credentials
* releases if desired

Repository caches and build workspaces can be recreated.

---

# 49. Cleanup

Build workspaces should be removed after deployments complete or fail.

Keep only a configurable number of successful releases per site.

Default:

```text
5
```

Failed deployments do not need full publish artifacts unless useful for debugging.

Repository cache remains.

Perform cleanup safely and never delete:

* current release
* rollback target currently being activated
* another active deployment workspace

---

# 50. Deleting a site

Deleting a site should require confirmation.

Deletion should:

* disable serving
* remove it from Chimney
* remove database records according to chosen retention semantics
* remove releases
* remove repository cache
* remove site-specific credentials if no longer used

Do not accidentally delete shared credentials.

Potentially allow:

```text
Delete site but retain deployment history
```

later.

Not required for MVP.

---

# 51. Non-goals

These are explicitly outside scope.

Do not implement:

* Docker
* Docker Compose
* Kubernetes
* Nomad
* backend service deployment
* long-running user application processes
* databases
* Redis
* managed Postgres
* cron jobs
* workers
* serverless functions
* preview deployments
* per-PR environments
* edge functions
* CDN
* DNS management
* Caddy management
* Proxmox management
* multi-node deployments
* autoscaling
* teams
* organizations
* RBAC
* billing
* usage metering
* project templates
* CI pipelines
* arbitrary pipeline stages
* marketplace/plugins
* Terraform generation
* Docker compatibility abstractions
* container abstraction layers
* Kubernetes-ready architecture

Do not add abstractions solely because they may theoretically support these features later.

Blank deploys static frontend projects.

---

# 52. Product philosophy

Use these principles while implementing Blank.

## Small

Prefer a straightforward implementation over infrastructure abstractions.

## Native

Use normal Linux concepts.

## Safe deployments

Never expose partially built sites.

## Fast rollback

Releases should be immutable and activation atomic.

## Transparent

Users should always be able to see what command Blank is running and what failed.

## Chimney-first

Serving behavior belongs to Chimney.

## Mise-first

Runtime/tool installation belongs to Mise.

## Git-first

Repositories remain the source of application code.

## DB-authoritative config

Once imported or configured, Blank's database is the runtime source of truth for Chimney configuration.

## Server-state first

Frontend data from Blank's API belongs in TanStack Query rather than ad-hoc global stores.

## URL-driven navigation

Important navigation and view state should use TanStack Router and the URL where appropriate.

## Tailwind-first styling

Use Tailwind consistently rather than mixing styling systems.

---

# 53. Suggested repository structure

Start simple.

Something like:

```text
blank/
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── state.rs
│   │
│   ├── auth/
│   ├── api/
│   ├── db/
│   ├── sites/
│   ├── git/
│   ├── mise/
│   ├── deployment/
│   ├── chimney/
│   ├── secrets/
│   └── web/
│
├── frontend/
│   ├── src/
│   │   ├── components/
│   │   ├── features/
│   │   ├── lib/
│   │   ├── routes/
│   │   ├── styles/
│   │   ├── main.tsx
│   │   └── router.tsx
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind config as required by the selected Tailwind version
│   └── tsconfig.json
│
├── migrations/
├── scripts/
│   └── install.sh
│
├── Cargo.toml
├── mise.toml
└── README.md
```

Do not split everything into multiple Rust crates immediately.

One application crate with clear modules is preferable until real boundaries emerge.

---

# 54. Frontend dependency guidance

Start with approximately:

```text
react
react-dom
typescript
vite
tailwindcss

@tanstack/react-router
@tanstack/react-query
```

Add:

```text
@tanstack/react-table
```

only when a table view materially benefits from it.

Use the current recommended Vite/Tailwind integration for the installed Tailwind version rather than assuming an older setup.

Inspect current package documentation before configuring it.

Avoid unnecessary dependencies.

Do not automatically add:

* Redux
* Zustand
* React Router
* Axios if `fetch` is sufficient
* Material UI
* Ant Design
* Chakra UI
* Bootstrap
* styled-components
* Emotion

If a tiny accessibility primitive package is useful, evaluate it separately and document why it was added.

---

# 55. Implementation phases

Implement in this order.

## Phase 1 — Foundation

Create:

* Rust application
* Axum server
* SQLite
* migrations
* configuration loading
* React/Vite frontend
* Tailwind
* TanStack Router
* TanStack Query
* frontend shell
* embedded frontend assets
* first-run admin setup
* login/logout/session handling

Definition of done:

Blank launches and an administrator can log into an empty dashboard.

---

## Phase 2 — Site CRUD

Create database models and API/UI for:

* adding site
* editing site
* deleting site
* repository URL
* branch
* project directory
* build settings
* domains

Use TanStack Router for site routes and TanStack Query for data/mutations.

Definition of done:

A user can create and configure a site record.

---

## Phase 3 — Git

Implement:

* repository cache
* public clone/fetch
* SSH deploy keys
* branch resolution
* commit resolution
* worktree creation/removal
* commit metadata

Definition of done:

Blank can check out a configured repository and commit into an isolated deployment workspace.

---

## Phase 4 — Chimney config import

Implement:

* detect project `chimney.toml`
* parse
* validate
* import to DB
* store hash
* store commit
* create default configuration if absent
* structured UI
* advanced TOML editor
* export TOML

Definition of done:

A repository with or without `chimney.toml` can produce a valid DB-backed Chimney site configuration.

---

## Phase 5 — Embedded Chimney

Integrate `chimney-core`.

Implement:

* startup from DB site state
* serving active release roots
* domain routing
* configuration updates
* site removal/update behavior
* HTTP/TLS as supported by Chimney

Definition of done:

Blank can serve a manually prepared static site through embedded Chimney.

---

## Phase 6 — Deployment engine

Implement the state machine and steps:

* fetch
* checkout
* Mise setup
* dependency installation
* build
* publish
* validation
* release creation
* activation
* cleanup

Definition of done:

Blank can deploy a real static frontend repository end-to-end.

---

## Phase 7 — Logs

Implement:

* deployment log persistence
* stdout/stderr capture
* SSE streaming
* deployment detail UI
* useful failure summaries
* efficient live log rendering

Definition of done:

Build progress can be watched live from the browser and reviewed afterwards.

---

## Phase 8 — Releases and rollback

Implement:

* immutable releases
* atomic activation
* release retention
* rollback
* cleanup

Definition of done:

A broken release can be rolled back instantly without rebuilding.

---

## Phase 9 — Environment variables

Implement:

* encrypted storage
* editing UI
* build injection
* secret redaction protections

Definition of done:

Vite/Astro/etc builds can safely receive production variables.

---

## Phase 10 — Automatic deploys

Implement:

* webhook endpoint
* GitHub signature validation
* branch filtering
* queueing
* duplicate prevention

Definition of done:

A push to `main` can automatically trigger a deployment.

---

## Phase 11 — Host installer

Implement:

* installer
* Blank user
* directories
* permissions
* Git requirement
* Mise installation
* systemd service
* first startup
* documentation

Definition of done:

Blank can be installed on a clean Debian/Ubuntu VM/LXC without Docker.

---

## Phase 12 — Polish

Improve:

* empty states
* loading states
* errors
* deployment UX
* log viewer
* route transitions
* mutation feedback
* mobile usability
* site status
* config-change warnings
* accessible forms
* destructive-action confirmation
* Tailwind design consistency

Do not add new product scope during this phase.

---

# 56. Initial MVP acceptance test

The first genuinely usable Blank version should pass this scenario.

Given a clean Linux VM/LXC:

1. install Blank
2. access `https://blank.example.com`
3. create the first administrator
4. add a Git repository
5. configure branch `main`
6. configure project directory if the frontend is in a monorepo
7. Blank detects `chimney.toml` if present
8. Blank imports the Chimney configuration
9. user can modify imported configuration in the UI
10. Blank uses Mise to install project build tools
11. Blank runs dependency installation
12. Blank runs the build
13. Blank publishes the selected output directory
14. Blank creates an immutable release
15. Blank atomically activates it
16. embedded Chimney serves the website
17. deployment logs are visible live
18. a second Git push creates another release
19. the previous site stays online throughout the build
20. if the new build fails, the existing site remains online
21. if the new deployment succeeds, activation is atomic
22. the user can roll back to the previous release
23. rollback does not require a rebuild
24. a repository `chimney.toml` changed after import is detected but does not overwrite dashboard configuration automatically
25. frontend navigation is handled by TanStack Router
26. API/server state is handled through TanStack Query
27. styling is consistently implemented with Tailwind CSS
28. the production host does not require a Node installation for Blank itself

If all of these work, Blank has achieved its initial goal.

---

# 57. Development instructions

Before implementing each major subsystem:

1. inspect the relevant upstream tool/API
2. understand its actual current behavior
3. implement against reality rather than assumptions

This is particularly important for:

* `chimney-core`
* Mise
* Git worktrees
* Chimney TLS/ACME
* runtime Chimney configuration changes
* Tailwind's current Vite setup
* TanStack Router's current APIs
* TanStack Query's current APIs

When a required capability appears missing from Chimney:

* verify that it is actually missing
* prefer improving Chimney itself if the functionality belongs there
* keep Blank's Chimney adapter thin

Do not silently replace Chimney features with custom implementations.

---

# 58. Code quality

Prioritize:

* understandable code
* strong typed state transitions
* clear errors
* transactional DB updates where needed
* filesystem safety
* minimal dependency count
* tests around dangerous path/release operations
* sensible React component boundaries
* predictable query keys
* typed route parameters
* consistent Tailwind styling

Tests are particularly important for:

* path traversal protection
* atomic activation
* rollback
* config import
* config hash-change detection
* webhook signature validation
* deployment state transitions
* interrupted deployment recovery
* API validation
* authentication/session behavior

Avoid enormous abstractions.

Prefer obvious code.

---

# 59. Documentation

README should explain:

## What Blank is

A small self-hosted deployment tool for static frontend sites.

## What Blank is not

A Docker platform, PaaS, or CI system.

## Requirements

* Linux
* Git
* Mise
* ports/network routing as appropriate

## Installation

Clean-host instructions.

## Deploying a site

Repository → build → domain → deployment.

## chimney.toml

Explain that:

* Blank detects it
* imports it
* DB becomes authoritative
* users can edit config in Blank
* later upstream changes are detected
* config can be exported again

## Frontend architecture

Document that the UI uses:

* React
* TypeScript
* Vite
* Tailwind CSS
* TanStack Router
* TanStack Query

and explain that TanStack packages should remain focused on useful server-state/routing concerns rather than turning into unnecessary abstraction.

## Security

Explicitly explain that repository build commands execute as the Blank service user.

## Backups

Explain what under `/var/lib/blank` must be preserved.

---

# 60. Final constraint

At every design decision ask:

> Does Blank need this to deploy and operate static frontend sites through Chimney?

If the answer is no, do not add it.

The ideal Blank installation should ultimately feel like:

```text
one Rust binary
+
SQLite
+
Git
+
Mise
```

with Chimney embedded inside the Rust process.

For development, the UI stack should remain equally focused:

```text
React
+
TypeScript
+
Vite
+
Tailwind CSS
+
TanStack Router
+
TanStack Query
```

Use TanStack for the useful bits, not as an excuse to add complexity.

That simplicity is a feature and should be preserved.

