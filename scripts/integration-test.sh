#!/usr/bin/env bash
#
# Runs the Go integration suite (build tag `integration`) against a throwaway
# PostgreSQL/TimescaleDB database.
#
# `make test-integration` and the `backend-integration` CI job both call this
# script, so the two cannot drift: what a developer runs locally is byte-for-byte
# what CI runs. It talks to the database container by name over the `infra`
# network, which is how the local shared stack (/var/www/infra) is arranged and
# how the CI job arranges its service container.
#
# Requires: docker, a running database container named $PG_CONTAINER on the
# `infra` network, and the compose `app` service buildable.

set -euo pipefail

PG_CONTAINER="${PG_CONTAINER:-postgres}"
PG_SUPERUSER="${PG_SUPERUSER:-postgres}"
DB_NAME="${INTEGRATION_DB:-profitofexile_integration}"
DB_USER="${INTEGRATION_DB_USER:-profitofexile}"
DB_PASSWORD="${INTEGRATION_DB_PASSWORD:-profitofexile}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

DATABASE_URL="postgresql://${DB_USER}:${DB_PASSWORD}@${PG_CONTAINER}:5432/${DB_NAME}?sslmode=disable"

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

psql_super() {
	docker exec -i "$PG_CONTAINER" psql -U "$PG_SUPERUSER" -v ON_ERROR_STOP=1 "$@"
}

in_app() {
	docker compose run --rm -T -e DATABASE_URL="$DATABASE_URL" app "$@"
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

# The inventory of tests a phase is required to have run, derived from the tree
# by FILE NAME (`*_integration_test.go`) rather than by build tag.
#
# Reading the name is the whole point. The failure this guards against is a
# `//go:build integration` line that no longer says `integration` — a typo, a
# `//go:build integration,!ci`, a stray space after `//` — and an inventory
# derived from that line would drop the mistyped file from the expected side as
# well as the actual side, agree with itself, and stay green. The file name is
# the one property that survives the mistake.
#
# Reading the name has a mirror blind spot — a correctly tagged file that is not
# named like one. assert_names_and_tags_agree below closes it.
#
# The phase split mirrors the `go test` invocations below: phase 1 runs
# ./internal/db/migrations/..., phase 2 runs everything else.
expected_tests() {
	local scope="$1"
	local -a files=()

	if [ "$scope" = migrations ]; then
		mapfile -t files < <(find ./internal/db/migrations -name '*_integration_test.go' | sort)
	else
		mapfile -t files < <(find . \
			\( -name node_modules -o -name target -o -name .git \) -prune -o \
			-name '*_integration_test.go' -not -path './internal/db/migrations/*' -print |
			sort)
	fi

	if [ ${#files[@]} -eq 0 ]; then
		return 0
	fi
	# TestMain is the one `func Test…` that is not a test — it is the package
	# entry point and never prints a verdict line, so requiring one would fail
	# the day somebody adds a fixture bootstrap.
	grep -hoE '^func Test[A-Za-z0-9_]*' "${files[@]}" |
		sed 's/^func //' | grep -vx TestMain | sort -u
}

# The two properties that make a file an integration test — the name and the
# build tag — must be present together or neither is checkable.
#
# expected_tests reads the NAME, deliberately, so a mistyped tag cannot delete a
# file from the expected side as well as the actual side. That leaves the mirror
# case wide open: a file carrying `//go:build integration` but NOT named
# `*_integration_test.go` never enters the expected inventory at all, so nothing
# notices when it stops running. Neither guard below can see it — it produces no
# SKIP line and no missing name — which is precisely the blind spot this script
# exists to not have.
#
# So assert set equality and fail on either direction. This runs before the
# database is touched, because it needs neither.
#
# The tag pattern is the exact whole line. Anything else — `//go:build
# integration,!ci`, `// go:build integration`, a trailing comment — is not a file
# the phases compile, so a file named like an integration test but tagged that
# way lands on the name side alone and is reported here rather than silently
# skipped everywhere.
integration_files_by_name() {
	find . \( -name node_modules -o -name target -o -name .git \) -prune -o \
		-name '*_integration_test.go' -print | sed 's|^\./||' | sort
}

# grep over an explicit `find` list rather than `grep -r --include`: the prune
# set then comes from one place, and the recursive-grep flags differ between GNU
# grep and the drop-in replacements developers alias `grep` to.
integration_files_by_tag() {
	local -a gofiles=()
	mapfile -t gofiles < <(find . \
		\( -name node_modules -o -name target -o -name .git \) -prune -o \
		-name '*.go' -print)
	[ ${#gofiles[@]} -gt 0 ] || return 0
	grep -lx -- '//go:build integration' "${gofiles[@]}" | sed 's|^\./||' | sort
}

assert_names_and_tags_agree() {
	local by_name by_tag only_name only_tag
	by_name="$(integration_files_by_name)"
	by_tag="$(integration_files_by_tag || true)"

	if [ "$by_name" = "$by_tag" ]; then
		echo "    naming and build tags agree on $(printf '%s\n' "$by_name" | grep -c .) integration files"
		return 0
	fi

	only_name="$(comm -23 <(printf '%s\n' "$by_name" | sed '/^$/d') <(printf '%s\n' "$by_tag" | sed '/^$/d'))"
	only_tag="$(comm -13 <(printf '%s\n' "$by_name" | sed '/^$/d') <(printf '%s\n' "$by_tag" | sed '/^$/d'))"

	echo >&2
	echo "FAIL: integration test files disagree between their name and their build tag." >&2
	if [ -n "$only_name" ]; then
		echo "      Named *_integration_test.go but NOT carrying '//go:build integration':" >&2
		echo "      these compile into the default build, or the tag is mistyped —" >&2
		echo "      either way the expected inventory demands tests that never run." >&2
		printf '%s\n' "$only_name" | sed 's/^/        /' >&2
	fi
	if [ -n "$only_tag" ]; then
		echo "      Carrying '//go:build integration' but NOT named *_integration_test.go:" >&2
		echo "      these run today but nothing requires them to — rename the file, or" >&2
		echo "      the guards below are blind to it going missing." >&2
		printf '%s\n' "$only_tag" | sed 's/^/        /' >&2
	fi
	return 1
}

# Fail on a green-but-empty run. assert_nothing_skipped catches a harness that
# reached the tests and bailed; this catches one that never reached them —
# `-tags integration` dropped from a phase, a mistyped build tag, a package moved
# out from under the phase's path. All three produce `[no test files]` or a
# smaller-than-expected set, zero SKIP lines, and exit 0.
#
# It asserts by NAME rather than by a floor on `=== RUN` lines, because a count
# is a blunt instrument for phase 2: that phase's package list is the whole
# module, so dropping `-tags integration` there still runs every unit test in the
# repository. Measured 2026-08-05 by doing exactly that: 1371 `=== RUN` lines
# tagged against 1281 untagged — a 7% dip, in a number that moves by more than
# that whenever somebody adds a table-driven case. Any floor loose enough not to
# churn is loose enough to pass a phase that ran no integration test at all. Per
# name there is no such window: TestSQLExclusionsMatchGoPredicates either ran or
# it did not, and the failure names the missing tests instead of a delta.
#
# `--- PASS:` rather than `=== RUN`: a run that logged the name and then died
# without a verdict is not a test that ran. Top-level results are unindented in
# `go test -v` output, so the anchored pattern cannot be satisfied by a subtest.
assert_expected_tests_ran() {
	local phase="$1" log="$2" scope="$3"
	local name missing=0 expected=0

	while read -r name; do
		[ -n "$name" ] || continue
		expected=$((expected + 1))
		if ! grep -qE "^--- PASS: ${name} " "$log"; then
			if [ "$missing" -eq 0 ]; then
				echo >&2
				echo "FAIL: ${phase} did not run every integration test it compiles." >&2
				echo "      Check the build tag on the file declaring each name below," >&2
				echo "      and that the phase still passes -tags integration." >&2
			fi
			echo "  missing: ${name}" >&2
			missing=$((missing + 1))
		fi
	done < <(expected_tests "$scope")

	if [ "$expected" -eq 0 ]; then
		echo >&2
		echo "FAIL: ${phase} — no *_integration_test.go files found to check against." >&2
		echo "      Either the naming convention changed or the checkout is incomplete;" >&2
		echo "      either way this script is no longer asserting anything." >&2
		return 1
	fi
	if [ "$missing" -gt 0 ]; then
		echo >&2
		echo "FAIL: ${missing} of ${expected} expected integration tests did not run in ${phase}." >&2
		return 1
	fi
	echo "    ${phase}: ${expected}/${expected} expected integration tests ran"
}

echo "==> Checking integration file naming against build tags"
assert_names_and_tags_agree

echo "==> Recreating ${DB_NAME}"
# Recreated rather than reused because phase 1 needs a virgin database:
# TestLeagueMigrationsPreserveLegacySchemaContract migrates DOWN to the
# pre-POE-119 schema, and 20260729120000_dedication_variant is forward-only — its
# down migration deliberately RAISEs. Against an already-migrated database that
# raise leaves golang-migrate dirty at 20260725001000, and every later test in
# the run (in every package, not just this one) fails with "Dirty database
# version". A virgin database has nothing to migrate down, so the guard never
# fires.
if ! psql_super -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname = '${DB_USER}'" | grep -q 1; then
	psql_super -d postgres -c "CREATE ROLE ${DB_USER} LOGIN PASSWORD '${DB_PASSWORD}'"
fi
psql_super -d postgres -c "DROP DATABASE IF EXISTS ${DB_NAME} WITH (FORCE)"
psql_super -d postgres -c "CREATE DATABASE ${DB_NAME} OWNER ${DB_USER}"

echo "==> Phase 1: migration tests (virgin database)"
in_app go test -tags integration -race -count=1 -v ./internal/db/migrations/... \
	2>&1 | tee "${LOG_DIR}/migrations.log"
assert_nothing_skipped "phase 1 (migrations)" "${LOG_DIR}/migrations.log"
assert_expected_tests_ran "phase 1 (migrations)" "${LOG_DIR}/migrations.log" migrations

echo "==> Applying migrations"
# Phase 1 happens to leave the schema migrated up, but that is a side effect of
# its last test rather than a contract. Apply explicitly so phase 2 does not
# depend on phase 1's finishing state.
in_app go run ./cmd/migrate up

echo "==> Phase 2: everything else (migrated database)"
# -p 1: all packages share the one database. Without it the packages that apply
# or roll back migrations (internal/league, cmd/migrate) interleave with the
# packages that read tables, and the suite fails on "Dirty database version"
# nondeterministically.
#
# The package list is built in checked steps rather than in one `$(go list ... |
# grep -v ...)` substitution, for three reasons.
#
#   - `go list` must see the same build tags as the `go test` below it. Measured
#     2026-08-05: an untagged `go list ./...` omits internal/db/migrations
#     entirely (its only .go file is behind `//go:build integration`, so the
#     package has no files to list), which made the `grep -v` that follows it a
#     no-op — it filtered a line that was never there. It produced the right
#     answer for the wrong reason, and it would have hidden any FUTURE
#     integration-only package by never listing it for phase 2 at all.
#   - An inlined `go list` that fails — a broken import, a bad module cache, a
#     compile error anywhere — expands to the empty string, and `go test` with no
#     package arguments falls back to the current directory: one package, no
#     integration files, exit 0.
#   - The exclusion is load-bearing rather than cosmetic (see the virgin database
#     note above), so a miss is checked for: a renamed or removed migrations
#     package would otherwise put phase 1's down-migration tests silently back
#     into phase 2's shared-database run.
in_app sh -c '
set -e
all=$(go list -tags integration ./...)
echo "$all" | grep -q "^profitofexile/internal/db/migrations$" || {
	echo "the migrations package did not match its exclusion pattern — phase 2 would rerun phase 1 against the shared database" >&2
	exit 1
}
pkgs=$(echo "$all" | grep -v "^profitofexile/internal/db/migrations$")
[ -n "$pkgs" ] || { echo "no packages left to test after excluding migrations" >&2; exit 1; }
go test -tags integration -race -p 1 -count=1 -v $pkgs
' 2>&1 | tee "${LOG_DIR}/suite.log"
assert_nothing_skipped "phase 2 (suite)" "${LOG_DIR}/suite.log"
assert_expected_tests_ran "phase 2 (suite)" "${LOG_DIR}/suite.log" rest

echo "==> Integration suite passed"
