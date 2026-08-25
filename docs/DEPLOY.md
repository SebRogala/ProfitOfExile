# Deployment

Status: current. Last verified 2026-07-26; the desktop <-> server ordering
section was added and verified 2026-08-25.

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

## Desktop <-> server ordering

The two halves ship on separate pipelines — the server on merge to `main`
(`deploy.yml`), the desktop on a `v-desktop-X.Y.Z` tag (`desktop.yml`) — so a
release that spans both has an order, and it is always the same one:

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
