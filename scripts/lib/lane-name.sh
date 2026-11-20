#!/usr/bin/env bash
# The one home of the lane-name rule. Sourced by scripts/lane-isolation.sh and
# scripts/lib/integration-env.sh; scripts/worktree-create.test.mjs pins the
# worktree-create.mjs pattern to LANE_NAME_PATTERN.
LANE_NAME_BODY='lane[a-z0-9]{1,20}'
LANE_NAME_PATTERN="^${LANE_NAME_BODY}\$"
