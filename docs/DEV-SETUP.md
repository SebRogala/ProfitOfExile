# Development setup: WSL + Windows

Status: current, verified 2026-09-04 against the reference machine. Scope: a
fresh machine to a running server, web UI, and the desktop app in dev mode.

## Shape

Three places, one repo:

| Where | What runs there | Why |
|---|---|---|
| WSL2, `/var/www/infra` | Traefik, Postgres, Redis, Mailpit, Mercure (shared by every local project) | one stack that mirrors Coolify; this repo runs no database of its own |
| WSL2, `/var/www/ProfitOfExile` | Go server + collector (air hot-reload), SvelteKit web UI, the desktop Rust/JS **test** containers | Docker-first; no Go or Node on the host |
| Windows, `C:\Users\<you>\Projects\poe-desktop` | a synced copy of `desktop/` under `npx tauri dev` | Tauri produces a Windows GUI app: it cannot run in a Linux container, and Windows npm/cmd cannot build from a `\\wsl$\` UNC path, so the files must live on a native Windows path |

The desktop app reaches the WSL server through Traefik at
`https://profitofexile.localhost`, the same host the browser uses.

## 1. WSL side

Prerequisites on the WSL distro: Docker Desktop with the WSL2 backend (the
reference machine runs Docker Desktop, engine 29.x; the infra README notes
Traefik must be v3.6+ for that engine), `git`, `make`, `rsync`,
`inotify-tools` (for `make desktop-watch`), and `mkcert` (infra certificates).

1. **Shared infra first.** Clone the `infra` repository (private, ask the
   owner for access) to `/var/www/infra` and follow its README: `make certs` (mkcert bundle), then
   `make up`. Its init script creates the `profitofexile` database and role on
   the first Postgres start; Mercure is part of that stack too, keyed with the
   same default JWT secret this repo's compose file assumes.
2. **Trust the mkcert CA on Windows** (infra README, "One-time per machine"):
   copy `rootCA.pem` out of WSL with a `.crt` extension and import it into
   *Trusted Root Certification Authorities* on the Local Machine. This one
   import covers the Windows browser, WebView2, and the desktop app's Rust HTTP
   client, which validates TLS through the Windows store (`reqwest` on its
   default `native-tls` backend, `schannel` in `desktop/src-tauri/Cargo.lock`).
3. **Clone this repo** to `/var/www/ProfitOfExile`. Other documents and the
   Makefile assume that path.
4. **`make up`.** Builds and starts `app`, `frontend`, and `collector` on the
   external `infra` network (the `desktop` service is only built on demand by
   the `desktop-*` targets). Every compose variable has a working default, so
   no `.env` is needed locally; `.env.example` lists what can be overridden.
   The server applies pending migrations itself on start
   (`cmd/server/main.go`), and the league migration seeds `runtime_config`
   with an active league, so a fresh database is usable immediately. It fills
   from poe.ninja on the collector's first tick; historical data is a
   prod-to-local copy, which is not documented in this repository.
5. **Open `https://profitofexile.localhost`.** Traefik routes `/api` to the Go
   server and everything else to the Vite dev server (both `docker-compose.yml`
   labels). The Traefik dashboard at `http://localhost:8080` shows whether the
   routers registered.

**Host naming, legacy note.** The infra stack's default for new projects is
`<name>.dev.localhost`, covered by one wildcard certificate with no per-host
work. This project predates that scheme and still uses the bare
`profitofexile.localhost` and `mercure.localhost`, which work only because both
are listed as explicit SANs in infra's `setup-certs.sh`. Moving to
`profitofexile.dev.localhost` is a deliberate change, not a setup step: it
touches both Traefik labels and `MERCURE_PUBLIC_URL` in `docker-compose.yml`,
the desktop defaults in `desktop/src-tauri/src/settings.rs`,
`desktop/src/lib/api.ts`, and `desktop/src/lib/components/TopBar.svelte`, the
`CORS_ORIGINS` default, and the Windows hosts entry below.

### Gates that run in WSL

```bash
make test                     # Go suite, race detector
make qa                       # Go + desktop Rust + desktop vitest + npm peer-range check
make desktop-check-windows    # type-checks the cfg(windows) half against x86_64-pc-windows-gnu
```

Run `make desktop-check-windows` before every Windows build: the Linux
`cargo check` and `cargo test` never compile the overlay hook, click-through,
or capture code (Overlay Guide, "Windows smoke checks").

`make desktop-test` needs the merc seed-art fixture once per clone:
`make merc-seed-art POE_SERVER_URL=https://profitofexile.top`. The PNGs are
GGG art and deliberately not in git
(`desktop/src-tauri/tests/fixtures/merc-seed-art/README.md`); a local server
has no icons to serve until its cache is seeded per [GEM-ICONS.md](GEM-ICONS.md),
which is why the fetch points at production.

## 2. Windows side

Install, in this order:

1. **Microsoft C++ Build Tools** with the *Desktop development with C++*
   workload (Tauri v2 prerequisites). Visual Studio 2022 Build Tools is what
   the reference machine has.
2. **Rust** via `rustup` (stable, the default MSVC host toolchain).
3. **Node.js LTS.** CI builds with `lts/*`; the reference machine runs v24.
4. **WebView2 Runtime.** Present on current Windows 10/11; the Tauri
   prerequisites page covers the case where it is not.

Two Windows-specific settings, both observed on the reference machine
(2026-03) and both required:

- **hosts file** (`C:\Windows\System32\drivers\etc\hosts`):
  `127.0.0.1 profitofexile.localhost`. Chromium and WebView2 resolve
  `*.localhost` internally; the app's Rust HTTP client goes through the
  Windows resolver, which does not, so without this line the desktop app
  cannot reach the local server even though the browser can.
- **Smart App Control** blocks unsigned Rust build scripts. Add an exclusion
  for the project directory (or turn it off), or `cargo` fails on the first
  build-script crate.

## 3. Sync WSL to Windows

In WSL, create `.env.local` (gitignored; the Makefile includes it):

```
DESKTOP_WIN_DIR=/mnt/c/Users/<you>/Projects/poe-desktop
```

Then:

```bash
make desktop-sync    # one-shot rsync of desktop/ to DESKTOP_WIN_DIR
make desktop-watch   # inotifywait loop: re-sync on every change (Ctrl+C to stop)
```

Both are `rsync --delete` excluding `node_modules`, `.svelte-kit`, `build`,
`target`, and `Cargo.lock`, so:

- the Windows copy keeps its own `node_modules`, build output, and lockfile;
  CI builds from the committed `Cargo.lock`, the Windows dev build resolves its
  own;
- anything you put inside `DESKTOP_WIN_DIR` that is not in `desktop/` is
  deleted on the next sync. Keep private scripts and captures outside it.

Sync is one-way. Edit in WSL; never edit the Windows copy.

## 4. Run the desktop app

On Windows, in `DESKTOP_WIN_DIR`:

```
npm ci
npx tauri dev
```

`npm ci`, not `npm install`: the CI workflow records an npm regression where
`npm install` silently skipped the `@tauri-apps/cli-win32-x64-msvc` optional
dependency and the build died on the missing native module. `npx tauri dev`
starts Vite on port 1420 (`tauri.conf.json` `devUrl`), builds the Rust crate,
and opens the app; Svelte changes hot-reload, Rust changes relink. The debug
exe lands at `src-tauri\target\debug\ProfitOfExile.exe`.

**Which server it talks to.** Unset, the build-time `POE_SERVER_URL` and
`VITE_SERVER_URL` default to `https://profitofexile.localhost`
(`settings.rs`, `api.ts`, `TopBar.svelte`). The Settings page persists a
`server_url` override at runtime, which is the way to point a dev build at
production without rebuilding. `APP_FINGERPRINT_SECRET` falls back to a fixed
dev-only salt (`fingerprint.rs`), so device identity works locally without CI
secrets; the server's `CORS_ORIGINS` default already allows
`http://localhost:1420` and `tauri://localhost`.

**Driving it without the game.** A second debug instance with its own WebView2
user-data folder and a remote-debugging port can be scripted from WSL through
`desktop/scripts/smoke-cdp.mjs`; the exact commands are in the Overlay Guide
under "Windows smoke checks". Kill that instance before any Rust rebuild, or
`tauri dev` cannot relink.

## Daily loop

```
WSL terminal 1:  cd /var/www/infra && make up; cd /var/www/ProfitOfExile && make up
WSL terminal 2:  make desktop-watch
Windows:         npx tauri dev            (in DESKTOP_WIN_DIR)
```

Server-side changes hot-reload through air inside Docker. Release builds never
happen on this machine: pushing a `v-desktop-*` tag builds and publishes on CI
([DEPLOY.md](DEPLOY.md), "Desktop release channels").
