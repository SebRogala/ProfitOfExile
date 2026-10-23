# Lane provisioning (POE-303)

Status: current implementation; live guarded acceptance belongs to the Supervisor.
The provisioning contract is Pipeforge's `lib/herdr-session-recipes.md`
§ Worktree provisioning. This target is called by guarded creation, not directly
as a substitute for that command:

```sh
make worktree-create LANE=laneexample BASE=dev
```

Prerequisites: Node, Git, Docker Compose, the running shared local infrastructure,
the existing local `profitofexile` PostgreSQL role, and built main containers
`profitofexile_app`, `profitofexile_frontend`, `profitofexile_collector`, and
`profitofexile_desktop` supplying their existing images. Export
`WORKTREE_DB_PASSWORD` with that role's local password through the caller's
environment. No primary environment file is copied. Pipeforge must be installed
at `/root/repos/pipeforge/plugin/bin/pipeforge`; PATH lookup is not used.
`PIPEFORGE_LANE_RESOURCE_HANDLE` must be supplied by guarded provisioning.

Passwords are percent-encoded once from their raw value, including dollar
characters. Template values use literal callback replacement; any remaining
dollars in Compose string values are doubled for Compose interpolation.
The private integration password file uses the same percent-encoded value.

Save these base `deliver` settings before guarded acceptance:

```json
{
  "worktreeCreate": {
    "target": "worktree-create",
    "arguments": { "LANE": "name", "BASE": "base" },
    "baseMode": "argument"
  },
  "worktreeResourceCommands": {
    "postgres-database": [
      "docker", "--host", "unix:///var/run/docker.sock",
      "exec", "-i", "postgres", "psql", "-U", "postgres"
    ]
  }
}
```

## Created resources

The target creates `.worktrees/<lane>` on new branch `<lane>` from the resolved
base commit. Lane names match `^lane[a-z0-9]{1,20}$`. Existing paths, branches,
databases, container names, or Compose members are refused; failed allocation
is retained rather than retried or deleted.

Two databases are created on the shared `postgres` container:
`profitofexile_<lane>` for the app and collector, and
`profitofexile_<lane>_integration` for integration tests. The separate dev DB
keeps the app out of the integration runner's destructive reset. Neither main
database (`profitofexile`, `profitofexile_integration`) is modified.

The Compose project is `profitofexile-<lane>`; its four container names are
`profitofexile_<lane>_{app,frontend,collector,desktop}`. API and frontend router
and service names are `profitofexile-<lane>-api` and
`profitofexile-<lane>-frontend`. The host is
`https://profitofexile-<lane>.dev.localhost`. The shared infra README documents
wildcard TLS for `*.dev.localhost`; this host uses that wildcard and differs
from main's explicitly covered `profitofexile.localhost`. No certificate or
shared infra configuration changes are needed by this target.

The standalone Compose template publishes no host ports, allocates no named or
anonymous volumes, and joins only the existing external `infra` network. Mutable
caches are bind mounts under the lane's `.worktree/`, owned by Pipeforge's
existing lane-boundary subtree. Existing images are reused without builds or
pulls. Image-declared volumes and unexpected Compose inventory are refused.
Database comments and container labels carry private allocation tokens.
Each DB/container has an intent before creation and an observation afterward;
the Compose group registers its exact observed members. The exact allocation
set is completed last. Git and lane/scratch boundaries are already registered
by the calling command.

The generated `.env` selects `docker-compose.lane.yml` and the lane project for
ordinary Compose calls within the lane. The rendered service database URLs are
fixed to the lane dev DB, so an exported main `DATABASE_URL` cannot override
them. The generated Vite config selects the lane host and proxies to its exact
app container name, avoiding main's shared `app` network alias. App migrations
run at creation. App, frontend and collector start; desktop is created but stays
stopped until used. Frontend dependencies install on startup. Creation does not
prove HTTP readiness, upstream data availability, or a populated market cache.

## Integration tests

Inside the lane, run:

```sh
make test-integration
```

Generated `.env.local` exports `INTEGRATION_DB` to the host integration script
before it sources `scripts/lib/integration-env.sh`. It also exports the lane
Compose selection and reads the URI-encoded password from the private
`.worktree/db-password` file. Thus the runner resets only the lane integration
DB and launches its Go tests with that DB URL. Calling the integration scripts
directly requires explicitly supplying the same environment; `.env` is read
by Compose, not by Bash. Do not override these lane settings with main's values.

When the requested base predates POE-303, creation replaces its recognized
legacy integration guard and adds missing lane-local ignore entries. These
appear as tracked lane diffs; the requested base commit is preserved. An unknown
guard is refused.

The full integration runner drops and recreates its database, changing the OID
and removing its allocation comment. **After a full integration run, guarded
removal needs owner-reviewed ownership reconciliation of that DB against fresh
evidence.** A retained token does not authorize blindly re-tagging a changed
database. The targeted runner uses the existing DB without recreating it.
This target does not change the integration runner or bypass ownership checks.

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

## Diagnostics, dry run and removal

After validating the handle and the observed scratch identity, every step is
logged to `<registered lane-scratch locator>/worktree-create.log` (mode 0600),
normally `/tmp/pipeforge-lanes-<uid>/<operationId>/worktree-create.log`.
Child failures retain status, stdout and stderr, with handles, known tokens,
passwords and database URLs redacted. Logs survive failed make calls even when
the caller drops stderr. Invalid handles or changed scratch identities report
to stderr only because no validated log location exists.

```sh
LANE=lanexyz BASE=dev DRY_RUN=1 PIPEFORGE_LANE_RESOURCE_HANDLE=fake node scripts/worktree-create.mjs
LANE=lanexyz BASE=dev DRY_RUN=1 PIPEFORGE_LANE_RESOURCE_HANDLE=fake WORKTREE_DB_PASSWORD='synthetic-$NAME' node scripts/worktree-create.mjs
LANE=lanexyz BASE=dev DRY_RUN=1 PIPEFORGE_LANE_RESOURCE_HANDLE=fake WORKTREE_DB_PASSWORD='synthetic-$&' node scripts/worktree-create.mjs
make -n worktree-create LANE=lanexyz BASE=dev
```

Dry run resolves the base, checks lane/path/branch availability and template
substitution, then prints the planned inventory. It verifies that both rendered
service DB URLs contain no interpolation dollars and decode to the supplied
password, and checks literal replacement and Compose dollar escaping separately.
It reports only `passwordRenderingVerified: true`, never the password or URL.
Without a supplied password it checks a dummy value. It ignores the fake handle
and contacts neither Docker, PostgreSQL nor registration. It does not prove
runtime setup, actual Compose parsing or diagnostics.

Pipeforge's `herdr-worktree --action remove` owns removal: it verifies ownership
and usage, drops registered PostgreSQL databases through the pinned argv above,
removes registered Docker members, and completes the Compose group without
broad `compose down`. Therefore no `worktree-teardown` target is provided.
After all lane sessions finish, the Supervisor must stop the exact lane
containers (for example `docker compose stop` from the lane with its generated
Compose selection), then use guarded removal and its closure evidence. Active
containers/DB clients and unresolved process scans refuse removal. Preserve
all resources until that closure; do not manually drop databases or remove
containers/volumes/worktrees. Recovery after partial setup requires review of
the original record and fresh ownership evidence; create is not a resume tool.
No refresh target is provided.
