#!/bin/sh
# Acceptance fixtures for POE-442. Scratch Git stays outside the checkout.
set -eu

source_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/poe-size-gate.XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT HUP INT TERM
fail=0

lines() { awk -v n="$1" 'BEGIN { for (i=0; i<n; i++) print "// fixture" }'; }
commit() { git add -- .; git -c user.name=SizeGate -c user.email=size-gate@example.invalid commit -qm fixture; }
fixture() {
	mkdir -p "$scratch/$1"
	cd "$scratch/$1"
	git init -q -b dev
	mkdir -p desktop/src-tauri/src/ssot desktop/src-tauri/src/temple desktop/src-tauri/src/lab desktop/src
	: > desktop/src-tauri/src/ssot.rs
	: > desktop/src-tauri/src/ssot/lab.rs
	: > desktop/src-tauri/src/lab_scan.rs
	: > desktop/src-tauri/src/font_fixture.rs
	: > desktop/src-tauri/src/temple/mod.rs
	: > desktop/src-tauri/src/lab/mod.rs
	if [ -f "$source_root/scripts/size-gate.sh" ]; then
		gate=$source_root/scripts/size-gate.sh
	else
		# The old gate resolves its root from its own path; keep its bytes intact.
		mkdir scripts
		cp "$source_root/scripts/rust-size-gate.sh" scripts/rust-size-gate.sh
		gate=$PWD/scripts/rust-size-gate.sh
	fi
}
branch() { commit; git checkout -qb feature; }
run_gate() {
	status=0
	output=$(SIZE_GATE_BASE="${base_ref:-dev}" sh "$gate" 2>&1) || status=$?
}
expect() {
	name=$1 expected_status=$2 diagnostic=$3
	if [ "$status" -eq "$expected_status" ] && printf '%s\n' "$output" | grep -qxF "$diagnostic"; then
		echo "ok   - $name"
	else
		echo "FAIL - $name (exit $status, expected $expected_status; expected: $diagnostic)"
		printf '%s\n' "$output"
		fail=1
	fi
}
excluded() {
	if [ "$status" -eq 0 ] && ! printf '%s\n' "$output" | grep -F "$2"; then
		echo "ok   - $1"
	else
		echo "FAIL - $1 (exit $status; unexpected diagnostic: $2)"
		printf '%s\n' "$output"
		fail=1
	fi
}

fixture crossing
lines 1500 > crossing.go
branch
lines 1501 > crossing.go
commit
run_gate
expect 'keep:happy Go 1500 to 1501 crossing fails' 1 'crossing.go: 1500 -> 1501 production lines crosses 1500'

fixture already-over
lines 1600 > desktop/src-tauri/src/large.rs
branch
lines 1601 > desktop/src-tauri/src/large.rs
commit
run_gate
expect 'keep:happy already-over Rust warns' 0 'warn: desktop/src-tauri/src/large.rs: 1600 -> 1601 production lines (over 1500; plan a split WI or a reviewed size-gate: intrinsic marker)'

fixture new-ts
branch
lines 1501 > desktop/src/new.ts
commit
run_gate
expect 'keep:happy new TS over limit fails' 1 'desktop/src/new.ts: new file with 1501 production lines > 1500'

fixture svelte-style
{ lines 1400; echo '<style>'; lines 300; echo '</style>'; } > desktop/src/View.svelte
branch
{ lines 1450; echo '<style>'; lines 300; echo '</style>'; } > desktop/src/View.svelte
commit
run_gate
excluded 'keep:happy Svelte style excluded' desktop/src/View.svelte:

fixture rust-tests
{ lines 1400; printf '#[cfg(test)]\nmod tests {\n'; lines 898; echo '}'; } > desktop/src-tauri/src/tests.rs
branch
{ lines 1400; printf '#[cfg(test)]\nmod tests {\n'; lines 948; echo '}'; } > desktop/src-tauri/src/tests.rs
commit
run_gate
excluded 'keep:happy Rust test module excluded' desktop/src-tauri/src/tests.rs:

fixture go-tests
branch
lines 2000 > large_test.go
commit
run_gate
excluded 'keep:happy Go test file excluded' large_test.go:

fixture intrinsic
{ echo '// size-gate: intrinsic — fixture data'; lines 1599; } > desktop/src/data.ts
branch
lines 1 >> desktop/src/data.ts
commit
run_gate
expect 'keep:happy intrinsic already-over prints reason' 0 'intrinsic: desktop/src/data.ts: 1601 production lines — fixture data'

fixture intrinsic-crossing
{ echo '// size-gate: intrinsic — fixture data'; lines 1499; } > desktop/src/data.ts
branch
lines 1 >> desktop/src/data.ts
commit
run_gate
expect 'keep:happy intrinsic marker cannot exempt crossing' 1 'desktop/src/data.ts: 1500 -> 1501 production lines crosses 1500'

# A missed forbidden import silently lets a feature-internal dependency land.
import_case=0
for import in 'use crate::{
    temple::run::Session,
};' 'use crate::temple::{slice::TempleSlice, run::Session};'; do
import_case=$((import_case + 1))
fixture "forbidden-import-$import_case"
branch
printf '%s\n' "$import" > desktop/src-tauri/src/ssot.rs
commit
run_gate
expect 'keep:critical multiline internal import fails' 1 'ssot -> feature internals'
done

# Missing nested or same-line imports silently admits feature dependencies.
for location in ssot.rs ssot/nested/path.rs; do
import_case=0
for import in 'use crate::temple::run::Session;' 'use crate::temple::run::Session; use std::fmt;' 'use std::fmt; use crate::temple::run::Session;' 'use crate::temple::{slice::TempleSlice}; fn leak() { crate::temple::run::Session::new(); }'; do
import_case=$((import_case + 1))
fixture "forbidden-statement-$location-$import_case"
branch
mkdir -p desktop/src-tauri/src/ssot/nested
printf '%s\n' "$import" > "desktop/src-tauri/src/$location"
commit
run_gate
expect "keep:critical $location statement $import_case rejects silent dependency" 1 'ssot -> feature internals'
done
done

import_case=0
for import in 'use crate::{temple::slice::TempleSlice, mercenary::MercenarySlice};' 'use crate::temple::{slice::TempleSlice};' 'use crate::temple::{
    slice::TempleSlice,
};'; do
import_case=$((import_case + 1))
fixture "allowed-import-$import_case"
branch
printf '%s\n' "$import" > desktop/src-tauri/src/ssot.rs
commit
run_gate
excluded 'keep:critical allowed slice import passes boundary' 'ssot -> feature internals'
done

# Dropping the non-use check would silently admit feature-internal dependencies.
for location in ssot.rs ssot/nested/path.rs; do
fixture "forbidden-non-use-$location"
branch
mkdir -p desktop/src-tauri/src/ssot/nested
printf 'use crate::temple::{slice::TempleSlice};\nfn leak() { crate::temple::run::Session::new(); }\n' > "desktop/src-tauri/src/$location"
commit
run_gate
expect 'keep:critical non-use feature internal path fails' 1 'ssot -> feature internals'
done

# An invalid base must fail instead of silently trusting an empty change set.
fixture missing-base
branch
base_ref=missing-ref
run_gate
expect 'keep:critical missing base fails closed' 1 'size-gate: cannot resolve base missing-ref'
unset base_ref

# Local dev already contains the crossing: a local-dev default would miss it.
fixture landed-crossing
lines 1500 > landed.go
commit
git init -q --bare "$scratch/origin.git"
git remote add origin "$scratch/origin.git"
git push -q -u origin dev
lines 1501 > landed.go
commit
status=0
output=$(unset SIZE_GATE_BASE; sh "$gate" 2>&1) || status=$?
expect 'keep:critical landed crossing compares remote dev' 1 'landed.go: 1500 -> 1501 production lines crosses 1500'

fixture untracked
branch
lines 1501 > desktop/src/untracked.ts
run_gate
expect 'keep:happy untracked TS over limit fails' 1 'desktop/src/untracked.ts: new file with 1501 production lines > 1500'

exit "$fail"
