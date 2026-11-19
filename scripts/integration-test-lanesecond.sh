#!/usr/bin/env bash
# Run one package against the exclusively leased lanesecond integration database.
# Usage: ./scripts/integration-test-lanesecond.sh <package> [go test args…]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
lane_db=profitofexile_lanesecond_integration
if [ -n "${INTEGRATION_DB+set}" ] && [ "${INTEGRATION_DB}" != "$lane_db" ]; then
	echo "FAIL: this runner only targets the leased $lane_db database." >&2
	exit 1
fi
export INTEGRATION_DB="$lane_db"
exec ./scripts/integration-test-targeted.sh "$@"
