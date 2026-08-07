#!/usr/bin/env bash
#
# Starts the throwaway database that scripts/integration-test.sh talks to, and
# blocks until it is accepting connections.
#
# Both `backend-integration` (quality.yml) and `validate` (deploy.yml) call this.
# validate is what gates the Coolify deploy webhook, so the two must agree about
# the container name, network and image — a divergence there means the deploy
# gate tests a different database than the PR gate did. Keeping the setup in one
# script is what stops that drift.
#
# Container name and network: the container is named `postgres` on the `infra`
# network so the compose default DATABASE_URL resolves unchanged, mirroring the
# local shared stack (/var/www/infra/docker-compose.yml).
#
# Image: timescale/timescaledb, matching the local stack, and it has to.
# 20260312200000_create_timescaledb_extension is the first migration, so plain
# postgres fails the whole suite at setup rather than running a reduced one. The
# image installs the extension into template1, so the non-superuser role the
# tests connect as inherits it.
#
# The database is left virgin — no migrations applied. Phase 1 of
# integration-test.sh requires that; see the "Recreating" comment there.
#
# Requires: docker, and the `infra` network already created by the caller.

set -euo pipefail

PG_CONTAINER="${PG_CONTAINER:-postgres}"
PG_NETWORK="${PG_NETWORK:-infra}"
PG_IMAGE="${PG_IMAGE:-timescale/timescaledb:latest-pg16}"

docker run -d --name "$PG_CONTAINER" --network "$PG_NETWORK" \
	-e POSTGRES_USER=postgres \
	-e POSTGRES_PASSWORD=postgres \
	--health-cmd "pg_isready -U postgres" \
	--health-interval 5s \
	--health-timeout 3s \
	--health-retries 20 \
	"$PG_IMAGE"

for _ in $(seq 1 60); do
	status="$(docker inspect -f '{{.State.Health.Status}}' "$PG_CONTAINER")"
	if [ "$status" = healthy ]; then
		echo "==> ${PG_CONTAINER} healthy"
		exit 0
	fi
	sleep 2
done

echo "${PG_CONTAINER} never became healthy" >&2
docker logs "$PG_CONTAINER" >&2
exit 1
