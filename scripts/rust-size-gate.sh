#!/bin/sh
# Desktop Rust size and boundary gates (POE-341, audit H2/H3/H4).
# Prints nothing and exits 0 on a clean tree; exits 1 naming the first failure.
# Lower a ceiling in the same change that lands a split.
set -u
cd "$(dirname "$0")/../desktop/src-tauri/src" || exit 1

# Production lines: every line outside a top-level `#[cfg(test)]` followed by
# `mod NAME {` ... first column-0 `}`. A `#[cfg(test)]` on anything else (fn,
# `mod x;`, use) counts as production, attribute included.
count() {
	awk '
skip { if ($0 ~ /^}/) skip=0; next }
pend { pend=0; if ($0 ~ /^(pub(\([a-z]+\))? )?mod [A-Za-z_0-9]+ *\{/) { skip=1; next } c+=2; next }
/^#\[cfg\(test\)\]$/ { pend=1; next }
{ c++ }
END { print c+0 }
' "$1"
}

check() {
	n=$(count "$1")
	[ "$n" -le "$2" ] || { echo "$1: $n production lines > $2"; exit 1; }
}

check ssot.rs 1765
check ssot/lab.rs 53

# temple must not reach merc internals (H4)
! grep -rn 'crate::mercenary::' temple/ || { echo "temple -> mercenary import"; exit 1; }
# ssot may name only slice types from features (H2). Matches one line at a
# time: a multi-line `use crate::{temple, ...}` import evades it.
grep -rnE 'crate::(temple|mercenary)::' ssot*.rs ssot/ 2>/dev/null \
	| grep -vE '::(slice::TempleSlice|MercenarySlice|sync::MercSyncStatus)' && { echo "ssot -> feature internals"; exit 1; }
# lab reads only the frame the read key hands it (POE-391)
! grep -rn 'capture_screen' lab_scan.rs lab/ font_*.rs || { echo "lab code grabs the screen outside read_hotkey"; exit 1; }
# only settings/ and persist_settings may write the settings file (H3)
! grep -rn 'settings::save(' --include='*.rs' . | grep -v '^./settings/' | grep -v 'fn persist_settings' \
	|| { echo "settings::save outside settings/ and persist_settings"; exit 1; }
exit 0
