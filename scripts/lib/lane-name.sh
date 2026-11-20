#!/usr/bin/env bash
# The one home of the lane-name rule. Sourced by scripts/lane-isolation.sh,
# scripts/lib/integration-env.sh and scripts/worktree-create.sh.
LANE_NAME_BODY='lane[a-z0-9]{1,20}'
LANE_NAME_PATTERN="^${LANE_NAME_BODY}\$"
