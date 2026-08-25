# Deployment

Status: current. Last verified 2026-07-26; the desktop <-> server ordering
section was added and verified 2026-08-25, and the desktop release channels
section 2026-08-25 (beta channel not yet exercised by a real tag; the client-side dual-manifest check ships with POE-203 WI-2).

Canonical for: how `main` reaches production, why the deploy is path-filtered, and
what a green pipeline does and does not tell you.

## Shape

One repository, two deployed services, one `Dockerfile` with two targets
(`server`, `collector`). Both run on a single shared VPS behind Coolify.

`.github/workflows/deploy.yml` ("Test & Deploy") runs on push to `main` and on
manual dispatch. It does three things in order: decide what changed, validate,
then ask Coolify to deploy.

`.github/workflows/quality.yml` runs separately and does not deploy.

## Why the deploy is path-filtered

Without filters every push to `main` would redeploy **both** services, including
for a docs-only commit. That is the cost the filters exist to avoid, and it is a
deliberate choice — not an accident to be optimised away.

The filters must match each binary's real dependency set. Derive them, do not
guess:

```
docker compose exec -T app go list -deps ./cmd/server    | grep '^profitofexile/'
docker compose exec -T app go list -deps ./cmd/collector | grep '^profitofexile/'
```

As of 2026-07-26:

- **server** imports every package under `internal/` — including
  `internal/collector` — so its filter is `internal/**` plus `cmd/server/**`,
  `frontend/**`, `Dockerfile`, `go.mod`, `go.sum`. A collector-only change
  deploying the server is **correct**, not waste: that code is compiled into the
  server binary.
- **collector** imports only `internal/{collector,db,league,mercure,price}`, so
  its filter is narrower.

**Re-derive the filters whenever a binary gains an import.** A filter that lags
the import graph is the failure documented below.

`deploy-collector` runs sequentially after `deploy-server`, not in parallel. The
prod host has ~3.7 GB RAM and two concurrent Go builds peak around 2 GB each; run
together they get OOM-killed at the linker step.

## What a green run actually means

**A green "Test & Deploy" does not mean production is running that commit.** Two
independent reasons:

1. **The deploy job may have been skipped.** `deploy-server` runs only when the
   `server` filter matched. A docs-only push correctly skips it — and so does a
   push the filter *should* have matched but didn't. Both look identical from
   outside: a green check.
2. **The deploy step is fire-and-forget.** It is a single `curl` to Coolify's
   deploy API. That returns as soon as Coolify *accepts* the request, not when the
   container swaps. The job can go green in seconds while the actual build and
   swap take minutes — or never happen, if Coolify's build fails downstream.

This is accepted deliberately. Verifying convergence would mean polling the
running image from CI, and the manual check below is cheap enough at this
project's release cadence. **If a deploy matters, verify it by hand.**

## Verifying a deploy landed

`PROD_HOST` and `SERVER_SERVICE_ID` are placeholders — this is a public
repository, so the real host alias and Coolify service ids live in the private
ops notes, not here. Set them once, then the commands below paste as-is (angle
brackets would be read by the shell as redirects):

```
PROD_HOST=...           # see private ops notes
SERVER_SERVICE_ID=...

ssh "$PROD_HOST" "docker ps --format '{{.Status}} | {{.Image}}' -f name=$SERVER_SERVICE_ID"
```

Look the service up by its stable id prefix, never by a full container name:
Coolify appends a deploy suffix that rotates on every rebuild, so a pasted
full name goes stale immediately.

The image tag is the deployed commit SHA. Compare it against `git rev-parse
origin/main`. `Up <n> minutes` should be small if the deploy just ran.

`/api/health` exposes a `version` field intended for exactly this, but it reads
`"dev"` in production: the `GIT_SHA` build arg is passed by the GitHub Actions
*validation* build, and Coolify — which performs the real build — does not pass
it. Passing `GIT_SHA` in the Coolify build configuration would make
`curl -s https://profitofexile.top/api/health` a one-line deploy check. Not done;
recorded here as the cheapest path if that ever becomes worth it.

## Desktop release channels

`desktop.yml` triggers on `v-desktop-*` and serves two channels off one tag
pattern:

- **Stable — `v-desktop-X.Y.Z`.** A normal GitHub release carrying the
  installer, the standalone exe, and `latest.json`. `/releases/latest` resolves
  to it, which is the endpoint pinned in `tauri.conf.json`, so every device is
  offered it.
- **Beta — `v-desktop-X.Y.Z-beta.N`.** Any tag whose version carries a semver
  prerelease suffix — `X.Y.Z-<prerelease>` — is a beta-channel tag; `-beta.N`
  is the convention, and the workflow matches the general form so an `-rc.1`
  tag or a typo fails closed into beta rather than shipping as stable. The same
  build, published as a GitHub **prerelease** so `/releases/latest` skips it
  entirely and stable devices never see it. The job additionally re-publishes
  that build's `latest.json` onto a rolling release under the fixed tag
  `desktop-beta`, giving beta devices a constant manifest URL:
  `https://github.com/SebRogala/ProfitOfExile/releases/download/desktop-beta/latest.json`.
  The `desktop-beta` release is itself a prerelease, and its `latest.json` is
  overwritten on every beta tag; the binaries stay on the versioned release the
  manifest's `url` points at.

  The `desktop-beta` **git tag** is created at default-branch HEAD by the first
  beta run and never moves afterwards — only the release assets hanging off it
  roll. That is harmless (nothing reads the tag's commit) and is written down
  here so nobody "fixes" it later.

`tauri.conf.json` sets `bundle.targets` to `["nsis"]` rather than `"all"`.
This is a hard requirement of the beta channel, not a preference: the WiX/MSI
bundler runs the version through a `convert_version` step that rejects
non-numeric semver prerelease identifiers, so a `-beta.N` version fails the
whole `tauri build` before NSIS is reached. Nothing was lost — the workflow has
only ever uploaded and published the NSIS installer and the standalone exe; the
MSI was built and discarded.

The version written into `latest.json` is the tag with `v-desktop-` stripped,
so a beta manifest advertises `X.Y.Z-beta.N`. That suffix is load-bearing: the
Tauri updater compares semver, and `1.2.0-beta.1` sorts *below* `1.2.0`, which
is what lets a beta device roll forward onto the stable release rather than
stranding on the beta manifest. A beta device must therefore check both manifests
and install the higher version.

That comparison has to be done by the client, not by the updater plugin's
endpoint list. `tauri-plugin-updater` treats `endpoints` as **failover, not
fan-out**: it walks the list and stops at the first endpoint that returns a
parseable manifest (`updater.rs` around line 496), so a two-entry list would
only ever report whichever manifest answered first — a stale beta manifest
would mask a newer stable one. The client must therefore issue two separate
`check()` calls, each against a single distinct endpoint, and compare the two
returned versions by semver itself.

### Who is on the beta channel, and what "hidden" means in a public repo

A device is on the beta channel when its server-side role is `editor` or
`admin`; the same role unlocks the hidden desktop modules (currently the
mercenary triage module) via `GET /api/device/me`. Promotion is a server-side
operation — no build, no config, no restart on the tester's side:

```
docker exec <server-container> /promote list
docker exec <server-container> /promote <fingerprint-prefix> editor "<alias>"
```

The tester finds their short device id in the app (Ctrl+Shift+F11 → identify
dialog) and sends it to you. A running app picks the new role up on its next
entitlements refresh (startup, then every 30 minutes); it does not need a
reinstall.

This is **hiding, not securing**, and the repository is **public**:

- The hidden module's code ships in every build and its source is on GitHub.
  Anyone can find it; the gate only keeps it out of the UI and the stable
  update stream. Do not put anything behind this gate that must stay secret.
- `generate_release_notes: true` publishes the commit subjects of every tag,
  prerelease included. A beta's release notes are public — treat them as such.
- Never commit, tag, or write into a release note, an issue, or a doc a
  tester's fingerprint, alias, or name. Promotion commands live in shell
  history and the database only. (AGENTS.md carries the general rule; this is
  its beta-channel instance.)

## Desktop <-> server ordering

The two halves ship on separate pipelines — the server on merge to `main`
(`deploy.yml`), the desktop on a `v-desktop-X.Y.Z` or `v-desktop-X.Y.Z-beta.N`
tag (`desktop.yml`) — so a release that spans both has an order, and it is
always the same one:

**The server must be live in production BEFORE the desktop tag that calls a new
API is pushed.** A desktop build released first talks to an endpoint that is not
there yet. Since both are pushed from the same `git push origin main --tags`,
"merged" is not "live": the Coolify swap takes minutes and the deploy step is
fire-and-forget (see *What a green run actually means* above). Push the server
commit, verify it landed, then push the tag.

**Concretely, for the mercenary support vocabulary:** the server must ship no
later than a change to
`desktop/src/lib/mercenaries/__fixtures__/mercenary-stats.json`. That fixture is
the single source both sides derive their family list from —
`internal/mercenary/families.go` is generated from it (and `families_test.go`
re-derives it, so the two cannot drift silently), and the desktop's
`vocab.rs` parses the same file. A desktop released first knows families the
server's `knownFamilies` map does not, and every template upload naming one is
refused. That is a degradation rather than an outage — the shared icon pool
simply does not learn the new families — and it is visible, not silent: the
upload response carries `rejected_unknown_family` and the served corpus carries
`known_family_count`. `families.go` states the same rule at its declaration.

## When you need a manual deploy

Trigger with:

```
gh workflow run deploy.yml --ref main
```

Needed whenever a change must reach production but touches no filtered path:

- A change to `.github/**` only — including a fix to the filters themselves. The
  workflow's own trigger paths do not include `.github/**`, so such a merge
  starts no run at all.
- A commit already merged whose deploy was skipped by a since-corrected filter.
  Filters evaluate the *current push's* changed files, never history, so fixing a
  filter does not retroactively ship anything.

This is expected to be rare — on the order of once in ten PRs — and manual
dispatch is the deliberate answer rather than building machinery around it.

## The incident this document exists for

On 2026-07-26 commit `c5c612f` added six entries to
`internal/gemicon/gem-icon-urls.json`, the `go:embed`ded icon map. "Test & Deploy"
went green **in 11 seconds** and shipped nothing.

The workflow triggered (its trigger paths include `internal/**`) but the `server`
filter listed packages individually — `cmd/server/**`, `internal/server/**`,
`internal/db/**`, `internal/lab/**` — and `internal/gemicon/**` was absent. So
`server` evaluated false, `deploy-server` was skipped, and the run passed.

Six of the ten `internal/` packages were affected: `gemicon`, `device`, `league`,
`mercure`, `price`, `trade`. A Go change to any of them was built, tested, passed,
and silently never shipped. `validate` still ran (it gates on `go`, which matches
`**/*.go`), so the pipeline looked entirely healthy. An earlier task shipped only
because it happened to touch `internal/lab` and `internal/server`.

Two lessons, both encoded above: derive filters from the import graph rather than
hand-maintaining a list, and treat a green pipeline as "Coolify was asked", never
as "production is running this commit".
