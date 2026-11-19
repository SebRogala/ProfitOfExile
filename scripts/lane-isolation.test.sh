#!/usr/bin/env bash
# Exercises scripts/lane-isolation.sh against throwaway git worktrees and a fake
# `docker` on PATH that keeps databases in a text file. Needs git and bash only;
# contacts no Docker daemon or PostgreSQL. Run: bash scripts/lane-isolation.test.sh
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
script="$repo/scripts/lane-isolation.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
check() { # check <description> <command...>
	local desc=$1; shift
	if "$@"; then echo "ok   - $desc"; else echo "FAIL - $desc"; fail=1; fi
}

mkdir -p "$work/bin"
cat >"$work/bin/docker" <<'FAKE'
#!/usr/bin/env bash
# Fake docker: databases live in $FAKE_DBS as "name<TAB>comment" lines.
echo "$*" >>"$FAKE_LOG"
case "$1 $2" in
	"image inspect") echo "sha256:fake-${5%%:*}" ;;
	"container ls") ;;
	"compose "*) ;;
	"exec -i")
		[[ -z ${FAKE_PSQL_DOWN:-} ]] || { echo 'Error response from daemon: container postgres is not running' >&2; exit 1; }
		sql=${*: -1}
		name=$(sed -E 's/.*(datname = |DATABASE )["'\'']([a-z0-9_]+)["'\''].*/\2/' <<<"$sql")
		case $sql in
			SELECT*) awk -F '\t' -v n="$name" '$1 == n { print "x" $2 }' "$FAKE_DBS" ;;
			CREATE*) printf '%s\t\n' "$name" >>"$FAKE_DBS" ;;
			COMMENT*) comment=$(sed -E "s/.* IS '(.*)'$/\1/" <<<"$sql")
				awk -F '\t' -v n="$name" -v c="$comment" 'BEGIN { OFS = "\t" } $1 == n { $2 = c } { print }' "$FAKE_DBS" >"$FAKE_DBS.new"
				mv "$FAKE_DBS.new" "$FAKE_DBS" ;;
			DROP*) awk -F '\t' -v n="$name" '$1 != n' "$FAKE_DBS" >"$FAKE_DBS.new"; mv "$FAKE_DBS.new" "$FAKE_DBS" ;;
		esac ;;
	*) echo "fake docker: unexpected $*" >&2; exit 1 ;;
esac
FAKE
chmod +x "$work/bin/docker"
export PATH="$work/bin:$PATH" FAKE_LOG="$work/docker.log" FAKE_DBS="$work/dbs"
unset WORKTREE_DB_PASSWORD

fresh_project() { # fresh_project <dir> <lane dir...>: primary checkout plus linked worktrees
	local primary=$1; shift
	: >"$FAKE_DBS"; : >"$FAKE_LOG"
	git init -q -b dev "$primary"
	cp "$repo/docker-compose.worktree.yml" "$primary/"
	git -C "$primary" add docker-compose.worktree.yml
	git -C "$primary" -c user.name=t -c user.email=t@t commit -qm base
	for lane in "$@"; do git -C "$primary" worktree add -q -b "$lane" "$primary/.worktrees/$lane" dev; done
}
in_dir() { (cd "$1" && shift && "$@"); }

# keep:happy — two lanes get disjoint Compose projects, databases and hosts.
p="$work/happy"
fresh_project "$p" poe-414 pf2-POE-415
in_dir "$p/.worktrees/poe-414" "$script" setup >/dev/null
in_dir "$p/.worktrees/pf2-POE-415" "$script" setup >/dev/null
expected_dbs=$(printf '%s\n' \
	"profitofexile_lanepoe414	lane-isolation:$p/.worktrees/poe-414" \
	"profitofexile_lanepoe414_integration	lane-isolation:$p/.worktrees/poe-414" \
	"profitofexile_lanepf2poe415	lane-isolation:$p/.worktrees/pf2-POE-415" \
	"profitofexile_lanepf2poe415_integration	lane-isolation:$p/.worktrees/pf2-POE-415")
check "each lane creates its own dev and integration database, marked with its checkout" \
	test "$(cat "$FAKE_DBS")" = "$expected_dbs"
check "each lane's .env selects its own Compose project and integration database" \
	test "$(tail -n +2 "$p/.worktrees/poe-414/.env")$(tail -n +2 "$p/.worktrees/pf2-POE-415/.env")" = \
	"$(printf 'COMPOSE_FILE=docker-compose.lane.yml\nCOMPOSE_PROJECT_NAME=profitofexile-lanepoe414\nINTEGRATION_DB=profitofexile_lanepoe414_integration')$(printf 'COMPOSE_FILE=docker-compose.lane.yml\nCOMPOSE_PROJECT_NAME=profitofexile-lanepf2poe415\nINTEGRATION_DB=profitofexile_lanepf2poe415_integration')"
lane_compose="$p/.worktrees/poe-414/docker-compose.lane.yml"
check "the lane compose file names only lane containers, its own host and its own dev database" \
	test "$(grep -Eo 'container_name: [a-z0-9_]+|Host\(`[^`]+`\)|@postgres:5432/[a-z0-9_]+' "$lane_compose" | sort -u | tr '\n' ' ')" = \
	'@postgres:5432/profitofexile_lanepoe414 Host(`profitofexile-lanepoe414.dev.localhost`) container_name: profitofexile_lanepoe414_app container_name: profitofexile_lanepoe414_collector container_name: profitofexile_lanepoe414_desktop container_name: profitofexile_lanepoe414_frontend '
check "setup migrates the lane's dev database through the lane Compose project" \
	grep -qxF "compose --project-directory $p/.worktrees/poe-414 -p profitofexile-lanepoe414 --profile collector -f docker-compose.lane.yml run --rm -T app go run ./cmd/migrate up" "$FAKE_LOG"

# keep:happy — a repeated setup reuses the lane's resources.
before=$(cat "$FAKE_DBS")
check "a second setup of the same lane succeeds" bash -c '(cd "$1" && "$2" setup) >/dev/null' _ "$p/.worktrees/poe-414" "$script"
check "a second setup creates no further database" test "$(cat "$FAKE_DBS")" = "$before"

# keep:happy — teardown removes exactly the lane's databases and generated files.
in_dir "$p/.worktrees/poe-414" "$script" teardown >/dev/null
check "teardown drops only that lane's databases" \
	test "$(cut -f1 "$FAKE_DBS" | tr '\n' ' ')" = 'profitofexile_lanepf2poe415 profitofexile_lanepf2poe415_integration '
check "teardown removes the lane's generated env and compose file" \
	bash -c '! ls -a "$1" | grep -qxE "\.env|\.env\.local|docker-compose\.lane\.yml|\.worktree"' _ "$p/.worktrees/poe-414"

# keep:critical — teardown in the main checkout would drop data; it must refuse.
p="$work/main"
fresh_project "$p" poe-414
printf 'profitofexile\t\nprofitofexile_integration\t\n' >"$FAKE_DBS"
check "teardown in the main checkout exits non-zero" bash -c '! (cd "$1" && "$2" teardown) 2>/dev/null' _ "$p" "$script"
check "teardown in the main checkout issues no DROP and stops no containers" \
	bash -c '! grep -Eq "DROP|compose" "$1"' _ "$FAKE_LOG"

# keep:critical — a database marked by someone else (v1 guarded lane, another
# directory with the same slug) is never shared by setup nor dropped by teardown.
for action in setup teardown; do
	p="$work/foreign-$action"
	fresh_project "$p" poe-414
	printf 'profitofexile_lanepoe414\tpipeforge-allocation:other\n' >"$FAKE_DBS"
	check "$action refuses a dev database owned by another allocation" \
		bash -c '! (cd "$1" && "$2" "$3") 2>/dev/null' _ "$p/.worktrees/poe-414" "$script" "$action"
	check "$action leaves the foreign database and its marker unchanged" \
		test "$(cat "$FAKE_DBS")" = "$(printf 'profitofexile_lanepoe414\tpipeforge-allocation:other')"
	check "$action runs no migration, DROP or COMMENT against it" \
		bash -c '! grep -Eq "DROP|COMMENT|migrate" "$1"' _ "$FAKE_LOG"
done

# keep:critical — with postgres unreachable, teardown must not report the lane's
# databases removed: lane-close cleanup trusts that result and the databases leak.
p="$work/psql-down"
fresh_project "$p" poe-414
in_dir "$p/.worktrees/poe-414" "$script" setup >/dev/null
rc=0
out=$(cd "$p/.worktrees/poe-414" && FAKE_PSQL_DOWN=1 "$script" teardown 2>&1) || rc=$?
check "teardown exits non-zero when psql cannot read the lane databases" test "$rc" -ne 0
check "teardown keeps the lane's env files when psql cannot read the lane databases" \
	test -f "$p/.worktrees/poe-414/.env" -a -f "$p/.worktrees/poe-414/.env.local"
check "teardown prints no removal when psql cannot read the lane databases" \
	bash -c '! grep -q "removed" <<<"$1"' _ "$out"

# keep:critical — an unmarked integration database beside an absent dev database
# belongs to another runner (a leased v1 lane with the same slug); claiming it
# lets two runners reset one database, and a later teardown drops it.
for action in setup teardown; do
	p="$work/unmarked-$action"
	fresh_project "$p" poe-414
	printf 'profitofexile_lanepoe414_integration\t\n' >"$FAKE_DBS"
	check "$action refuses an unmarked integration database when the lane has no dev database" \
		bash -c '! (cd "$1" && "$2" "$3") 2>/dev/null' _ "$p/.worktrees/poe-414" "$script" "$action"
	check "$action leaves the unmarked integration database unchanged and creates no database" \
		test "$(cat "$FAKE_DBS")" = "$(printf 'profitofexile_lanepoe414_integration\t')"
	check "$action issues no CREATE, COMMENT or DROP when it refuses the unmarked database" \
		bash -c '! grep -Eq "CREATE|COMMENT|DROP" "$1"' _ "$FAKE_LOG"
done

exit "$fail"
