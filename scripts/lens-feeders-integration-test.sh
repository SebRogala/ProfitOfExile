#!/usr/bin/env bash
# Run the POE-474 Lens exceptional-feeder integration selection against the
# exclusively leased lanepf2b integration database. It never falls back to the
# full integration suite.
# Usage: INTEGRATION_DB=profitofexile_lanepf2b_integration ./scripts/lens-feeders-integration-test.sh
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
lane_db=profitofexile_lanepf2b_integration
if [ "$#" -ne 0 ]; then
	echo "FAIL: this runner takes no arguments; its test selection is fixed." >&2
	exit 1
fi
if [ "${INTEGRATION_DB-}" != "$lane_db" ]; then
	echo "FAIL: this runner requires INTEGRATION_DB=$lane_db exactly; other databases are refused." >&2
	exit 1
fi
export INTEGRATION_DB
exec ./scripts/integration-test-targeted.sh ./internal/lab \
	-run '^(TestKeepCriticalLensExceptionalFeeders|TestExceptionalSupportPrices|TestKeepHappyExceptionalSupportsSharedSnapshot|TestExceptionalSupportCache)'
