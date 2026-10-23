#!/usr/bin/env bash
# Proves the local collector is opt-in (POE-305): without the Compose profile
# `collector`, neither `docker compose` nor `make up` (or a target that falls
# back to it) would start it. Starts nothing: runs only `docker compose config`
# and `make -n`. Run from anywhere: scripts/compose-profile.test.sh
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0
check() { # check <description> <command...>
	local desc=$1; shift
	if "$@"; then echo "ok   - $desc"; else echo "FAIL - $desc"; fail=1; fi
}

default_services=$(docker compose config --services)
profile_services=$(docker compose --profile collector config --services)

check "default services exclude collector" \
	bash -c '! grep -qx collector <<<"$1"' _ "$default_services"
check "default services still include app and frontend" \
	bash -c 'grep -qx app <<<"$1" && grep -qx frontend <<<"$1"' _ "$default_services"
check "--profile collector includes collector" \
	bash -c 'grep -qx collector <<<"$1"' _ "$profile_services"

# A dry-run line starts the collector if it activates the profile or names the
# collector service as an up/start/run/restart target.
starts_collector='(--profile[ =]collector|COMPOSE_PROFILES=[^ ]*collector|(up|start|run|restart|create)( [^ ]+)* collector( |$))'

# Every recursive make in the Makefile must be exactly `$(MAKE) up`, so no
# fall-back can reach `up-collector` or any other target.
only_plain_up() { ! grep -E $'^\t' Makefile | grep -oE '\$\(MAKE\) [^ ;|&)`]+' | grep -vxF '$(MAKE) up' | grep -q .; }
check 'every $(MAKE) fall-back targets exactly `up`' only_plain_up

# make runs a recipe line containing $(MAKE) even under -n, so the app probe
# executes and the fall-back expands only when it fails — i.e. only when app is
# down. A project name with no containers makes the probe fail every time, so
# the expansion is checked whether or not the real app is running.
export COMPOSE_PROJECT_NAME=poe305-profile-check-absent
for target in up build; do
	out=$(make -n "$target" 2>&1)
	check "make -n $target prints no command that starts the collector" \
		bash -c '! grep -Eq "$1" <<<"$2"' _ "$starts_collector" "$out"
done
check "make -n build expands its fall-back to the plain up" \
	bash -c 'grep -qxF "docker compose up -d --build" <<<"$1"' _ "$(make -n build 2>&1)"

exit "$fail"
