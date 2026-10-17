#!/usr/bin/env bash
# Run one package against the existing throwaway integration database.
# Usage: ./scripts/integration-test-targeted.sh <package> [go test args…]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

unset DATABASE_URL
source scripts/lib/integration-env.sh

refuse_package() {
	echo "FAIL: package rule requires exactly one package under ./internal/ or ./cmd/; migrations and recursive patterns are refused." >&2
	exit 1
}

[ "$#" -gt 0 ] || refuse_package
package="$1"
shift

# Literal components prevent traversal and recursive patterns from reaching the
# migrations package through an alternate spelling.
[[ "$package" =~ ^\./(internal|cmd)/[A-Za-z0-9_-]+(/[A-Za-z0-9_-]+)*/?$ ]] || refuse_package
case "$package" in
	./internal/db/migrations|./internal/db/migrations/*) refuse_package ;;
esac
[ -d "$package" ] || refuse_package
resolved_package="$(realpath -e -- "$package")"
case "$resolved_package" in
	"$REPO_ROOT/internal/db/migrations"|"$REPO_ROOT/internal/db/migrations/"*) refuse_package ;;
	"$REPO_ROOT/internal/"*|"$REPO_ROOT/cmd/"*) ;;
	*) refuse_package ;;
esac

# Parse flag arity so an additional package cannot hide among test arguments.
# Values belong to their flags (for example, the regex after -run). Additional
# flags can use -flag=value; bare flags below have known Go test arity.
test_args=("$@")
while [ "$#" -gt 0 ]; do
	case "$1" in
		-run|-bench|-benchtime|-timeout|-cpu|-parallel|-shuffle|-skip|-list|-count|\
		-tags|-p|-exec|-o|-vet|-gcflags|-asmflags|-ldflags|-mod|-modfile|\
		-overlay|-pkgdir|-toolexec|-buildmode|-buildvcs|-compiler|-installsuffix|\
		-coverprofile|-covermode|-coverpkg|-outputdir|-cpuprofile|-memprofile|\
		-memprofilerate|-blockprofile|-blockprofilerate|-mutexprofile|\
		-mutexprofilefraction|-trace|-fuzz|-fuzztime|-fuzzminimizetime)
			[ "$#" -ge 2 ] || refuse_package
			shift 2
			;;
		-C|-C=*|-args|-args=*|--) refuse_package ;;
		-*=*|-json|-failfast|-short|-fullpath|-cover|-benchmem|-v|-race|\
		-msan|-asan|-a|-n|-x|-work|-trimpath|-linkshared|-modcacherw)
			shift
			;;
		*) refuse_package ;;
	esac
done

database_exists="$(psql_super -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = '${DB_NAME}'")"
if [ "$database_exists" != 1 ]; then
	echo "FAIL: integration database is absent; run ./scripts/integration-test.sh first." >&2
	exit 1
fi

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

echo "==> Applying migrations"
in_app go run ./cmd/migrate up

echo "==> Running targeted integration tests"
in_app go test -tags integration -race -count=1 -v "$package" "${test_args[@]}" \
	2>&1 | tee "${LOG_DIR}/targeted.log"
assert_nothing_skipped "targeted package" "${LOG_DIR}/targeted.log"
