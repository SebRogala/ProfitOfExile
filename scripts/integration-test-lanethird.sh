#!/usr/bin/env bash
# Run one package against the exclusively leased lanethird integration database.
# Usage: ./scripts/integration-test-lanethird.sh <package> [go test args…]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
lane_db=profitofexile_lanethird_integration
if [ -n "${INTEGRATION_DB+set}" ] && [ "${INTEGRATION_DB}" != "$lane_db" ]; then
	echo "FAIL: this runner only targets the leased $lane_db database." >&2
	exit 1
fi
export INTEGRATION_DB="$lane_db"
# The lane password lives where lane-setup wrote it (.env.local reads the same
# file); a caller that did not export it through make gets it here, unprinted.
if [ -z "${INTEGRATION_DB_PASSWORD+set}" ] && [ -r .worktree/db-password ]; then
	INTEGRATION_DB_PASSWORD="$(cat .worktree/db-password)"
	export INTEGRATION_DB_PASSWORD
fi
exec ./scripts/integration-test-targeted.sh "$@"
