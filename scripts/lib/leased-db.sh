#!/usr/bin/env bash
# Callers change to the checkout root before resolving its integration lease.
source "$(dirname "${BASH_SOURCE[0]}")/lane-name.sh"

resolve_leased_db() {
	local leased_line='' status=1 leased_db=${INTEGRATION_DB-}
	if [[ -e .env ]]; then
		leased_line=$(grep -m 1 '^INTEGRATION_DB=' .env 2>/dev/null) && status=0 || status=$?
		if ((status > 1)); then
			echo 'FAIL: cannot read checkout lease configuration.' >&2
			return 1
		fi
	fi
	if ((status == 0)); then
		leased_db=${leased_line#INTEGRATION_DB=}
		if [[ ${INTEGRATION_DB+set} && $INTEGRATION_DB != "$leased_db" ]]; then
			echo 'FAIL: INTEGRATION_DB conflicts with the checkout integration lease.' >&2
			return 1
		fi
	fi
	if [[ ! $leased_db =~ ^profitofexile_${LANE_NAME_BODY}_integration$ ]]; then
		echo 'FAIL: select a leased lane integration database through lane-setup or INTEGRATION_DB.' >&2
		return 1
	fi
	export INTEGRATION_DB="$leased_db"
}
