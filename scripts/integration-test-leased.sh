#!/usr/bin/env bash
# Run one package against the checkout's exclusively leased integration database.
# Usage: ./scripts/integration-test-leased.sh <package> [go test args…]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
source scripts/lib/leased-db.sh
resolve_leased_db
# The lane password lives where lane-setup wrote it (.env.local reads the same
# file); a caller that did not export it through make gets it here, unprinted.
if [ -z "${INTEGRATION_DB_PASSWORD+set}" ] && [ -r .worktree/db-password ]; then
	INTEGRATION_DB_PASSWORD="$(cat .worktree/db-password)"
	export INTEGRATION_DB_PASSWORD
fi
exec ./scripts/integration-test-targeted.sh "$@"
