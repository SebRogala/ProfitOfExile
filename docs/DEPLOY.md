# Deployment

Status: current. Last verified 2026-07-26.

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

`<prod-host>` and `<server-service-id>` are placeholders — this is a public
repository, so the real host alias and Coolify service ids live in the private
ops notes, not here.

```
ssh <prod-host> 'docker ps --format "{{.Status}} | {{.Image}}" -f name=<server-service-id>'
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
