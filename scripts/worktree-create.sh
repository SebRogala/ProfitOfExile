#!/usr/bin/env bash
# Creates lane worktree <primary>/.worktrees/$WORKTREE_NAME from $WORKTREE_BASE
# (default dev), then runs `pipeforge init --cwd <lane>`, which runs the
# project's laneSetup (`make lane-setup`). Name and base arrive only through the
# environment (see the Makefile worktree-create target), never as shell source.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib/lane-name.sh
source "$here/lib/lane-name.sh"

fail() { echo "worktree-create: $*" >&2; exit 1; }

name=${WORKTREE_NAME:-}
base=${WORKTREE_BASE:-}
[[ -n $base ]] || base=dev

[[ $name =~ $LANE_NAME_PATTERN ]] || fail "name must match $LANE_NAME_PATTERN (got '$name'); usage: make worktree-create name=<lane> [base=<ref>]"
[[ $base != -* ]] || fail "base must not start with '-' (got '$base')"
oid=$(git rev-parse --verify --quiet "${base}^{commit}") || fail "base '$base' does not resolve to a commit"

common=$(git rev-parse --path-format=absolute --git-common-dir)
primary=$(dirname "$common")
lane="$primary/.worktrees/$name"

[[ ! -e $lane ]] || fail "$lane already exists"
! git -C "$primary" show-ref --verify --quiet "refs/heads/$name" || fail "branch '$name' already exists"

mkdir -p "$primary/.worktrees"
git -C "$primary" worktree add -b "$name" "$lane" "$oid"

if ! pipeforge init --cwd "$lane"; then
	echo "worktree-create: pipeforge init failed; the worktree $lane is kept." >&2
	echo "worktree-create: resume isolation with 'make lane-setup' inside the lane or rerun 'pipeforge init --cwd $lane'." >&2
	exit 1
fi
