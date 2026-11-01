#!/usr/bin/env bash
# Current POE-322 committed rollover proof. Requires the existing migrated,
# exclusively leased lane database; does not provision, wipe or start daemons.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
test "${INTEGRATION_DB:-}" = profitofexile_lanesecond_integration || {
 echo 'FAIL: rollover requires the exclusively leased lanesecond integration database.' >&2
 exit 1
}
unset DATABASE_URL
source scripts/lib/integration-env.sh

collector_down() {
 docker compose ps collector
 if [[ -n "$(docker compose ps --status running -q collector)" ]]; then
  echo 'FAIL: real collector is running; stop and escalate.' >&2
  return 1
 fi
}
collector_down
# The helper image and shared module cache are pre-provisioned. No Compose
# dependency startup and no external market calls occur in this fixture test.
log="$(mktemp)"
trap 'rm -f "$log"; collector_down' EXIT
test "$INTEGRATION_DB" = profitofexile_lanesecond_integration
docker run --rm --network infra \
 -v "$PWD:/app" -v profitofexile_go-pkg:/root/go/pkg -w /app \
 -e DATABASE_URL="$DATABASE_URL" -e GOFLAGS=-buildvcs=false \
 profitofexile-app:latest \
 go test -tags integration -race -count=1 -v ./internal/server \
 -run '^TestCommittedRolloverRejectsStaleSubmits$' -timeout 3m 2>&1 | tee "$log"
collector_down
assert_nothing_skipped 'committed rollover' "$log"
grep -q '^--- PASS: TestCommittedRolloverRejectsStaleSubmits ' "$log"
grep -q 'RESTORED: exact runtime identity/revision/timestamp and registry; zero fixture rows; both fences released' "$log"
echo 'Committed rollover: 1/1 named scenario passed; no skipped tests; restoration verified.'
