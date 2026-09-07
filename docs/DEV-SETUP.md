# Development setup: WSL + Windows

Status: current, verified 2026-09-05 against the reference machine. Scope: a
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

Docker Desktop must have the distro ticked under Settings, Resources, WSL
integration ("Enable integration with my default WSL distro" covers the
default distro). A Docker Desktop upgrade can reset this. The symptom is
`docker` in WSL answering "could not be found in this WSL 2 distro" while the
engine runs fine on the Windows side; re-tick it and Apply & restart.
The same upgrade can leave the auto-restarted `traefik` container with a dead
Docker-socket mount (its log repeats "Cannot connect to the Docker daemon" and
every `*.localhost` host answers 404); `docker compose up -d --force-recreate
traefik` in `/var/www/infra` fixes it.

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
- **PowerShell execution policy.** A fresh machine's default (`Restricted`)
  refuses `npm.ps1` and `npx.ps1`, so `npm ci` and `npx tauri dev` fail from
  PowerShell with "running scripts is disabled on this system". Either
  `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` once, or run them from
  `cmd`, where `npm.cmd`/`npx.cmd` are used instead.

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
and `target`, so:

- the Windows copy keeps its own `node_modules` and build output, but builds
  from the committed `Cargo.lock`, the same one CI builds from. The lockfile
  was excluded until 2026-09-05; a fresh copy then resolved its own and
  floated to newer Tauri crates than the npm packages `package-lock.json`
  pins, which `tauri dev` refuses with "Found version mismatched Tauri
  packages". That message means the Windows copy has a stale or self-resolved
  lockfile: re-run `make desktop-sync`;
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

**Quick rebuilds, and why the dev build is not slow any more (2026-09-07).**
`npx tauri dev` is the fast loop. Its Rust build used to be unoptimized, and
the app's pixel loops — capture conversion, the temple anchor correlation, the
OCR crop prep — ran 10–20× slower than release: a full temple read measured
4247 ms on the dev build against 666 ms on release, same code, same PC. The
`[profile.dev]` in `src-tauri/Cargo.toml` now compiles dependencies at
`opt-level = 3` and this crate at `1`, which keeps incremental rebuilds quick
and the loops close to release. Check with the `Temple: read timings` line in
`app.log`; if `cheap detect` or `anchor` still sit an order of magnitude above
the release numbers in `docs/TEMPLE-LIFECYCLE.md`, raise the crate to `2`.

**Release build without the installer.** For a timing question or an A/B
against the installed app, build the synced copy in release: from WSL

```
make desktop-release-windows
```

syncs `desktop/` and runs `scripts/build-release.cmd` (double-clickable on
Windows too); the exe lands at `src-tauri\target\release\ProfitOfExile.exe`
with the frontend embedded, sharing `settings.json` and `app.log` with the
installed app. Close a running `ProfitOfExile.exe` first — Windows will not
overwrite a running binary and cargo fails at the link step with `failed to
remove file`. First build ~7 min cold, incremental afterwards. It is a
dev-salt device on prod unless `APP_FINGERPRINT_SECRET` is set (below).

**Which server it talks to.** The Rust default for `server_url` is the
build-time `POE_SERVER_URL` and the web side's is `VITE_SERVER_URL`
(`settings.rs`, `api.ts`); unset, both are `https://profitofexile.localhost`.
The Settings page persists a `server_url` override at runtime, and a dev build
also has the DEBUG/PROD button in the top bar, which flips between the local
server and the production target (`server-toggle.ts`). Two things about that
override are easy to trip over:

- **The settings file is shared.** A dev build and the installed release build
  have the same Tauri identifier, so both read and write
  `%AppData%\profitofexile\settings.json`. Flipping the dev build to DEBUG
  points the installed app at the local server too, until either of them
  writes the url back.
- **A server switch resets entitlements.** The local server and production
  hold different roles for the same device, so on a `server_url` change the
  app withdraws the hidden-module grant and asks the new server at once
  (`status.svelte.ts`): modules the new server never granted disappear, and
  the ones it did appear as soon as it answers.

The server's `CORS_ORIGINS` default already allows `http://localhost:1420`
and `tauri://localhost`.

### Build-time variables

CI builds every release with three variables from repository secrets
(`.github/workflows/desktop.yml`). A dev build compiled without them works,
but differently, and on a fresh machine the differences look like bugs:

| Variable | Set | Unset |
|---|---|---|
| `VITE_SERVER_URL` | the DEBUG/PROD button has a production target | once on DEBUG the button has nowhere to go back to: its tooltip says so and a click opens Settings, where a url can be typed. The url the app left is remembered (pref `devServerReturnUrl`), so after one flip from a url typed into Settings the button finds its way back on its own |
| `POE_SERVER_URL` | the Rust default `server_url` is production | the default is the local server; moot once `settings.json` carries an override |
| `APP_FINGERPRINT_SECRET` | the device id is the one the installed release build has on this PC | the id is hashed with a fixed dev salt (`fingerprint.rs`), so the dev build is a **different device** on every server: it starts as a plain `user` there and sees no hidden modules until that id is identified and promoted on its own ([DEPLOY.md](DEPLOY.md), "Who is on the beta channel"). The identify dialog (Ctrl+Shift+F11) says "dev-salt build" under the id in that case |

Set them as Windows *user* environment variables (System Properties, or
`[Environment]::SetEnvironmentVariable('NAME', 'value', 'User')` in
PowerShell), then open a new terminal before `npx tauri dev`. The two urls
are the production origin; the secret is the value the CI secret holds. Get
it from the owner and keep it out of the repo and out of anything under
`DESKTOP_WIN_DIR`. All three are read at compile time (`option_env!`,
`import.meta.env`), so changing one means a rebuild, which cargo and Vite do
on their own.

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
