#!/usr/bin/env bash
# Shared integration connection and helpers. Callers change to the repository
# root before sourcing this file.

PG_CONTAINER="${PG_CONTAINER:-postgres}"
PG_SUPERUSER="${PG_SUPERUSER:-postgres}"
DB_NAME="${INTEGRATION_DB:-profitofexile_integration}"
DB_USER="${INTEGRATION_DB_USER:-profitofexile}"
DB_PASSWORD="${INTEGRATION_DB_PASSWORD:-profitofexile}"

if [ "$DB_NAME" != profitofexile_integration ]; then
	echo "FAIL: database rule requires profitofexile_integration; other INTEGRATION_DB values are refused." >&2
	exit 1
fi

DATABASE_URL="postgresql://${DB_USER}:${DB_PASSWORD}@${PG_CONTAINER}:5432/${DB_NAME}?sslmode=disable"

psql_super() {
	docker exec -i "$PG_CONTAINER" psql -U "$PG_SUPERUSER" -v ON_ERROR_STOP=1 "$@"
}

# -buildvcs=false, module-wide via GOFLAGS rather than per invocation, because a
# per-invocation flag drifts: phase 1 does not need it (no main package under
# internal/db/migrations) and would sit there untouched until the day one lands.
#
# Observed 2026-08-07 on the first CI run of this script: phase 2 died with eight
# `error obtaining VCS status: exit status 128` — one per main package under
# cmd/. `go test` stamps VCS metadata into a main package's test binary, which
# shells out to git; in CI the checkout is owned by the runner and the container
# runs as another user, so git refuses the repository and the build fails. It
# passes locally only because the repository and the container user happen to
# agree there.
#
# Accepted trade-off: test binaries lose their VCS stamp. They are built, run and
# discarded inside this script — nothing reads the stamp — so the information has
# no consumer to lose.
in_app() {
	docker compose run --rm -T \
		-e DATABASE_URL="$DATABASE_URL" \
		-e GOFLAGS=-buildvcs=false \
		app "$@"
}

# Fail on a green-but-vacuous run. Every integration helper in this repository
# calls t.Skip when DATABASE_URL is unset or when the tables it reads are absent
# — the exact shape a misconfigured job takes — so a skipped integration test is
# a broken harness, not a passing one. A legitimately conditional skip added
# later has to be excluded here deliberately rather than by silence.
assert_nothing_skipped() {
	local phase="$1" log="$2"
	if grep -q -- '--- SKIP' "$log"; then
		echo >&2
		echo "FAIL: ${phase} skipped tests — the integration harness is not wired up." >&2
		grep -- '--- SKIP' "$log" >&2
		return 1
	fi
}

