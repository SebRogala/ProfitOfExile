#!/usr/bin/env bash
# Exercises scripts/integration-test-leased.sh and scripts/league-rollover-e2e.sh
# in throwaway fixture roots: copies of the scripts under test, a stub
# scripts/integration-test-targeted.sh that records what it receives, and a fake
# `docker` on PATH that logs every call. Contacts no Docker daemon or PostgreSQL
# and never reads the checkout's .env or lane password file.
# Run: bash scripts/integration-test-leased.test.sh
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
check() { # check <description> <command...>
	local desc=$1; shift
	if "$@"; then echo "ok   - $desc"; else echo "FAIL - $desc"; fail=1; fi
}

unset INTEGRATION_DB INTEGRATION_DB_PASSWORD INTEGRATION_DB_USER DATABASE_URL \
	COMPOSE_FILE COMPOSE_PROJECT_NAME PG_CONTAINER PG_SUPERUSER

mkdir -p "$work/bin"
cat >"$work/bin/docker" <<'FAKE'
#!/usr/bin/env bash
echo "$*" >>"$FAKE_LOG"
FAKE
chmod +x "$work/bin/docker"
export PATH="$work/bin:$PATH" FAKE_LOG="$work/docker.log" STUB_OUT="$work/stub"

# The generated-config marker is owned by lane-isolation.sh; read it from there.
marker=$(sed -nE 's/^generated="(.*)"$/\1/p' "$repo/scripts/lane-isolation.sh")
[[ -n $marker ]] || { echo "FAIL - lane-isolation.sh declares the generated-config marker"; exit 1; }

# fixture <dir> [lane]: copies of the scripts under test plus the targeted-runner
# stub; with a lane, a generated .env selecting that lane's integration database.
# Absent scripts under test are skipped so the cases below report them as failures.
fixture() {
	local root=$1 lane=${2:-} f
	mkdir -p "$root/scripts/lib"
	git init -q "$root"
	for f in scripts/integration-test-leased.sh scripts/lib/leased-db.sh scripts/lib/lane-name.sh \
		scripts/lib/integration-env.sh scripts/league-rollover-e2e.sh; do
		[[ ! -e $repo/$f ]] || cp -p "$repo/$f" "$root/$f"
	done
	cat >"$root/scripts/integration-test-targeted.sh" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "${INTEGRATION_DB-<unset>}" >"$STUB_OUT/db"
printf '%s\0' "$@" >"$STUB_OUT/argv"
if [ -z "${INTEGRATION_DB_PASSWORD+set}" ]; then echo unset
elif [ "$INTEGRATION_DB_PASSWORD" = "$(cat "$STUB_OUT/../expected-password")" ]; then echo match
else echo mismatch; fi >"$STUB_OUT/password"
STUB
	chmod +x "$root/scripts/integration-test-targeted.sh"
	if [[ -n $lane ]]; then
		printf '%s\nCOMPOSE_FILE=docker-compose.lane.yml\nCOMPOSE_PROJECT_NAME=profitofexile-%s\nINTEGRATION_DB=profitofexile_%s_integration\n' \
			"$marker" "$lane" "$lane" >"$root/.env"
	fi
}

# run <root> <script> [args...]: runs from outside the root with the caller's
# environment; sets rc, and leaves stdout/stderr in $work/out and $work/err.
run() {
	local root=$1 script=$2; shift 2
	: >"$FAKE_LOG"; rm -rf "$STUB_OUT"; mkdir "$STUB_OUT"
	rc=0
	(cd "$work" && "$root/$script" "$@") >"$work/out" 2>"$work/err" || rc=$?
}
refused() { # exit 1, exactly one stderr line starting FAIL:, no Docker call, stub not reached
	[[ $rc -eq 1 && $(wc -l <"$work/err") -eq 1 ]] && grep -q '^FAIL:' "$work/err" &&
		[[ ! -s $FAKE_LOG && ! -e $STUB_OUT/db ]]
}
stub_db_is() { [[ -e $STUB_OUT/db && $(cat "$STUB_OUT/db") == "$1" ]]; }
argv_is() { printf '%s\0' "$@" >"$work/expected-argv"; [[ -e $STUB_OUT/argv ]] && cmp -s "$work/expected-argv" "$STUB_OUT/argv"; }
password_seen() { [[ -e $STUB_OUT/password && $(cat "$STUB_OUT/password") == "$1" ]]; }
output_lacks_passwords() { ! grep -qF -e "$fallback" -e "$caller" "$work/out" "$work/err"; }

runner=scripts/integration-test-leased.sh
lease=profitofexile_lanepoe414_integration
foreign=profitofexile_laneother_integration

# keep:happy — AC-1: generated config selects the lease; package and arguments pass through unchanged.
fixture "$work/ac1" lanepoe414
run "$work/ac1" "$runner" ./internal/exchange -run 'TestPlays Page|TestOther' -count=1
check "with generated config and no caller database the targeted runner gets the leased database" stub_db_is "$lease"
check "the targeted runner gets the package and every argument unchanged" \
	argv_is ./internal/exchange -run 'TestPlays Page|TestOther' -count=1

# keep:critical — AC-2: a conflicting caller database would silently run against
# and corrupt another lane's database.
fixture "$work/ac2" lanepoe414
INTEGRATION_DB=$foreign run "$work/ac2" "$runner" ./internal/exchange
check "with generated config a different caller database is refused with one FAIL: line before Docker" refused

# keep:critical — AC-3: without generated config, an unset or shared database
# would silently select or modify shared data.
fixture "$work/ac3" ""
run "$work/ac3" "$runner" ./internal/exchange
check "without generated config an unset database is refused with one FAIL: line before Docker" refused
INTEGRATION_DB=profitofexile_integration run "$work/ac3" "$runner" ./internal/exchange
check "without generated config the shared integration database is refused with one FAIL: line before Docker" refused

# keep:happy — AC-4: without generated config an explicit lane database is used.
fixture "$work/ac4" ""
INTEGRATION_DB=$foreign run "$work/ac4" "$runner" ./internal/exchange
check "without generated config an explicit lane database reaches the targeted runner" stub_db_is "$foreign"

# keep:happy — AC-5: unset password falls back to the lane password file; a
# caller-set value (including empty) wins; neither value is printed.
fallback=synthetic-fallback-pw-Q7x
caller=synthetic-caller-pw-K3m
fixture "$work/ac5" lanepoe414
mkdir -p "$work/ac5/.worktree"
printf '%s' "$fallback" >"$work/ac5/.worktree/db-password"
for row in "unset|$fallback|an unset password falls back to the lane password file" \
	"set|$caller|a caller-set password wins over the lane password file" \
	"set||a caller-set empty password is passed through unreplaced"; do
	IFS='|' read -r mode expected label <<<"$row"
	printf '%s' "$expected" >"$work/expected-password"
	if [[ $mode == unset ]]; then
		run "$work/ac5" "$runner" ./internal/exchange
	else
		INTEGRATION_DB_PASSWORD=$expected run "$work/ac5" "$runner" ./internal/exchange
	fi
	check "$label" password_seen match
	check "runner output contains neither synthetic password ($label)" output_lacks_passwords
done

# keep:critical — AC-6: rollover with a foreign caller database would write
# fixture rows into another lane's data.
fixture "$work/ac6" lanepoe414
INTEGRATION_DB=$foreign run "$work/ac6" scripts/league-rollover-e2e.sh
check "rollover with generated config refuses a foreign caller database with one FAIL: line before Docker" refused

# keep:happy — AC-7: the replacement exists, the retired wrappers are gone and
# no tracked file names either retired lane.
retired=(lane"second" lane"third")
for id in "${retired[@]}"; do
	check "retired wrapper for ${id:0:5}…${id: -2} is deleted and untracked" \
		bash -c '[[ ! -e $1/$2 && -z $(git -C "$1" ls-files -- "$2") ]]' _ "$repo" "scripts/integration-test-$id.sh"
done
check "scripts/integration-test-leased.sh exists and is executable" test -x "$repo/scripts/integration-test-leased.sh"
check "scripts/lib/leased-db.sh exists" test -f "$repo/scripts/lib/leased-db.sh"
grep_rc=0
matches=$(git -C "$repo" grep -l -F -e "${retired[0]}" -e "${retired[1]}") || grep_rc=$?
check "the tracked file set can be searched" test "$grep_rc" -le 1
check "no tracked file contains a retired lane identifier" test "$grep_rc" -eq 1
[[ -z $matches ]] || sed 's/^/#   contains a retired identifier: /' <<<"$matches"

exit "$fail"
