#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

failures=0
assert_guard() {
	local name="$1" value="$2" expected="$3" output status=0
	output="$(INTEGRATION_DB="$value" bash -c 'source scripts/lib/integration-env.sh; printf "accepted:%s" "$DB_NAME"' 2>&1)" || status=$?
	if [ "$expected" = accept ]; then
		if [ "$status" -eq 0 ] && [ "$output" = "accepted:$value" ]; then
			printf 'PASS %s\n' "$name"
			return
		fi
	elif [ "$status" -eq 1 ] && [[ "$output" = 'FAIL: database rule '* ]] && [[ "$output" != *accepted:* ]]; then
		printf 'PASS %s\n' "$name"
		return
	fi
	printf 'FAIL %s: expected %s, status=%s, output=%q\n' "$name" "$expected" "$status" "$output" >&2
	failures=$((failures + 1))
}

assert_guard main_database_refused profitofexile refuse
assert_guard server_alias_refused profitofexile_server refuse
assert_guard maintenance_database_refused postgres refuse
assert_guard trailing_suffix_refused profitofexile_integration_x refuse
assert_guard missing_lane_id_refused profitofexile_lane_integration refuse
assert_guard uppercase_lane_refused profitofexile_LANE1_integration refuse
assert_guard empty_value_refused '' refuse
assert_guard semicolon_refused 'profitofexile_lane1_integration;' refuse
assert_guard space_refused 'profitofexile_lane1 integration' refuse
assert_guard command_substitution_refused 'profitofexile_$(id)_integration' refuse
assert_guard default_database_accepted profitofexile_integration accept
assert_guard lane_database_accepted profitofexile_lane298_integration accept
assert_guard shortest_lane_accepted profitofexile_lanea_integration accept
assert_guard longest_lane_accepted profitofexile_lane12345678901234567890_integration accept
assert_guard overlong_lane_refused profitofexile_lane123456789012345678901_integration refuse
[ "$failures" -eq 0 ]
