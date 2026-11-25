# Lane provisioning

Create a lane from the main checkout or any worktree:

```sh
make worktree-create name=laneexample            # base defaults to dev
make worktree-create name=laneexample base=main
```

`scripts/worktree-create.sh` checks that `name` matches `LANE_NAME_PATTERN`
in `scripts/lib/lane-name.sh` and that `base` resolves to a commit, refuses an
existing `.worktrees/<name>` path or `<name>` branch, runs
`git worktree add -b <name> .worktrees/<name> <base>` from the main checkout,
then runs `pipeforge init --cwd .worktrees/<name>`. Init runs the project's
`laneSetup` argv, `["make","lane-setup"]`, inside the new lane (§ Lane
isolation). When init fails the worktree is kept; rerun `make lane-setup` in
the lane or `pipeforge init --cwd <lane>`.

Prerequisites: Git, Bash, `pipeforge` on PATH, and lane-setup's own
prerequisites: Docker Compose, the running shared local infrastructure
(`postgres` container on the `infra` network), the local `profitofexile`
PostgreSQL role, and main images `profitofexile-<service>:latest` built with
`docker compose --profile collector build app frontend collector desktop` in
the main checkout. No primary environment file is copied.

## Lane isolation in an existing worktree (`make lane-setup`)

Pipeforge v2 creates the lane worktree itself and then runs the project's
`laneSetup` argv in it, with no lane argument. For that path, run from inside
a linked worktree at `.worktrees/<dir>`:

```sh
make lane-setup      # pipeforge laneSetup: ["make","lane-setup"]
make lane-teardown   # throwaway lanes only
```

The lane name is `<dir>` lowercased with non-alphanumerics dropped, prefixed
with `lane` when absent (`poe-414` gives `lanepoe414`); it must match
`LANE_NAME_PATTERN` in `scripts/lib/lane-name.sh`. It renders the same resources as § Created resources
(databases, Compose project, container names, host, `.worktree/` caches, `.env`,
`.env.local`, Vite config) from `docker-compose.worktree.yml`, reusing the main
images `profitofexile-<service>:latest`, and migrates the lane dev DB in a
one-off container, leaving none running; `make up` in the lane starts app and
frontend, never the collector. In the main checkout `lane-setup` does nothing and exits 0.

Each lane database carries the comment `lane-isolation:<checkout path>`.
Setup reuses a database with its own comment and refuses one with any other
comment; teardown drops only its own. The full integration runner recreates
the integration DB without a comment; both commands accept that. A lane made
by the v1 guarded provisioner (64-hex allocation labels) is left unchanged by
setup and refused by teardown. `WORKTREE_DB_PASSWORD` (default: the
documented local `profitofexile`) must use URL-unreserved characters.
Without Docker or PostgreSQL: `bash scripts/lane-isolation.test.sh`.

## Created resources

`make lane-setup` creates two databases on the shared `postgres` container:
`profitofexile_<lane>` for the app and collector, and
`profitofexile_<lane>_integration` for integration tests. The separate dev DB
keeps the app out of the integration runner's destructive reset. Neither main
database (`profitofexile`, `profitofexile_integration`) is modified.

The Compose project is `profitofexile-<lane>`; its four container names are
`profitofexile_<lane>_{app,frontend,collector,desktop}`. API and frontend router
and service names are `profitofexile-<lane>-api` and
`profitofexile-<lane>-frontend`. The host is
`https://profitofexile-<lane>.dev.localhost`, covered by the shared infra's
wildcard TLS for `*.dev.localhost`; no certificate or shared infra
configuration changes are needed.

`docker-compose.lane.yml`, rendered from `docker-compose.worktree.yml`,
publishes no host ports, allocates no named or anonymous volumes, and joins
only the existing external `infra` network. Mutable caches are bind mounts
under the lane's `.worktree/`. Existing images are reused without builds or
pulls.

The generated `.env` selects `docker-compose.lane.yml` and the lane project for
ordinary Compose calls within the lane. The rendered service database URLs are
fixed to the lane dev DB, so an exported main `DATABASE_URL` cannot override
them. The generated Vite config selects the lane host and proxies to its exact
app container name, avoiding main's shared `app` network alias. Setup migrates
the lane dev DB in a one-off container and leaves no container running; `make
up` in the lane starts app and frontend. Setup does not prove HTTP readiness,
upstream data availability, or a populated market cache.

Frontend dependencies install on every container start (`npm ci`, then
`svelte-kit sync`) into the lane's `.worktree/frontend_node_modules` bind
mount, never main's `frontend/node_modules`, so the cache follows the lane's
own lockfile after a branch changes it. The generated Vite config is
`.worktree/vite.config.mts`: outside `frontend/` there is no `"type": "module"`
package, so a `.ts` config there was bundled as CommonJS and failed to load the
ESM-only `@sveltejs/kit/vite`.

## Collector opt-in

The collector polls poe.ninja and GGG trade, sharing the machine's rate-limit
budget, so it sits behind the Compose profile `collector` and is not started by
lane setup, `make up` or a plain `docker compose up`. Start it only when the
Operator explicitly allows that start. To start it, run from the lane root:

```sh
docker compose --profile collector up -d collector
```

Stop it again with `docker compose --profile collector stop collector`.

## Integration tests

Inside the lane, run:

```sh
make test-integration
```

Generated `.env.local` exports `INTEGRATION_DB` to the host integration script
before it sources `scripts/lib/integration-env.sh`. It also exports the lane
Compose selection and reads the URI-encoded password from the private
`.worktree/db-password` file. Thus the runner resets only the lane integration
DB and launches its Go tests with that DB URL.
`./scripts/integration-test-leased.sh <package> [go test args…]` resolves the lease from lane-setup's generated `.env` and privately loads the lane password when unset.
Calling the other integration scripts directly requires explicitly supplying
the same environment; Compose reads `.env`, while those scripts do not.
Do not override these lane settings with main's values.

The guard regression test needs no Docker or database:

```sh
bash scripts/lib/integration-env.test.sh
```

It checks each refused name separately, accepted default/lane names, and lane
length boundaries. It asserts the guard diagnostic on refusal and the selected
DB on acceptance. No existing Make target runs this shell test.

## Shared-service limitations

The lane does not configure Redis; neither the main Compose file nor this
template supplies Redis settings. No Redis keys or shared service configuration
are changed. Mercure remains a shared hub with application topics that are not
lane-prefixed. This template leaves publisher/subscriber secrets empty; the
collector explicitly disables publishing when its secret is empty. Authenticated
live event checks are not provisioned. Supplying shared Mercure credentials
later requires accepting shared-topic cross-delivery; this target provides no
topic isolation. Shared Mailpit is not configured by the lane template.

## Removal

From inside the lane, `make lane-teardown` removes the lane's containers,
databases and generated env files (`.env`, `.env.local`,
`docker-compose.lane.yml`, `.worktree/`). Then remove the checkout from the
main checkout with `git worktree remove .worktrees/<name>` and delete its
branch when no longer needed. No refresh target is provided.

## v1-provisioned lanes

A lane created by the v1 guarded provisioner carries 64-hex allocation labels
in its `docker-compose.lane.yml`. `make lane-setup` leaves it unchanged and
`make lane-teardown` refuses it. Its removal is manual and owner-reviewed:
inspect its containers and databases against fresh evidence, stop and remove
them by hand, then `git worktree remove` the checkout.
