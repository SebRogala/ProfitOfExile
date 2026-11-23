#!/bin/sh
# Uniform 1500 production-line crossing gate (POE-442), plus H2/H3/H4/POE-391.
# Fail new files over 1500 or crossings from <=1500; already-over files warn.
# A plan touching an already-over file names a split WI or a reviewed
# `size-gate: intrinsic — <reason>` marker. The marker replaces the warning,
# never a crossing failure. Run from the caller's repository root.
set -u
base_ref=${SIZE_GATE_BASE:-origin/dev}
base=$(git merge-base HEAD "$base_ref" 2>/dev/null) || {
	echo "size-gate: cannot resolve base $base_ref"
	exit 1
}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/size-gate.XXXXXX") || exit 1
trap 'rm -rf -- "$scratch"' EXIT HUP INT TERM
failed=0

# Production lines: every line outside a top-level `#[cfg(test)]` followed by
# `mod NAME {` ... first column-0 `}`. A `#[cfg(test)]` on anything else (fn,
# `mod x;`, use) counts as production, attribute included.
count_rust() {
	awk '
skip { if ($0 ~ /^}/) skip=0; next }
pend { pend=0; if ($0 ~ /^(pub(\([a-z]+\))? )?mod [A-Za-z_0-9]+ *\{/) { skip=1; next } c+=2; next }
/^#\[cfg\(test\)\]$/ { pend=1; next }
{ c++ }
END { print c+0 }
' "$1"
}

count_go() { awk 'END { print NR+0 }' "$1"; }
count_ts() { awk 'END { print NR+0 }' "$1"; }
count_svelte() {
	awk '/<style([[:space:]>]|$)/ { style=1 }
style { if (/<\/style>/) style=0; next }
{ c++ }
END { print c+0 }' "$1"
}

git -c core.quotePath=false diff --name-status -M "$base" -- > "$scratch/changed" || exit 1
git -c core.quotePath=false ls-files --others --exclude-standard > "$scratch/untracked" || exit 1
while IFS= read -r path; do
	printf 'A\t%s\n' "$path"
done < "$scratch/untracked" >> "$scratch/changed"

while IFS="$(printf '\t')" read -r status old_path new_path; do
	case "$status" in
		D) continue ;;
		R*|C*) path=$new_path ;;
		*) path=$old_path ;;
	esac
	[ -f "$path" ] || continue
	case "$path" in
		*_test.go) continue ;;
		*.go) counter=count_go ;;
		desktop/src-tauri/src/*.rs) counter=count_rust ;;
		desktop/src/*.ts|desktop/src/*.svelte|frontend/src/*.ts|frontend/src/*.svelte)
			case "${path##*/}" in *.test.*|*.spec.*) continue ;; esac
			case "$path" in *.svelte) counter=count_svelte ;; *) counter=count_ts ;; esac ;;
		*) continue ;;
	esac
	head_count=$($counter "$path") || exit 1
	[ "$head_count" -gt 1500 ] || continue
	if git cat-file -e "$base:$old_path" 2>/dev/null; then
		git show "$base:$old_path" > "$scratch/base" || exit 1
		base_count=$($counter "$scratch/base") || exit 1
		if [ "$base_count" -le 1500 ]; then
			echo "$path: $base_count -> $head_count production lines crosses 1500"
			failed=1
		else
			reason=$(awk 'match($0, /size-gate: intrinsic —[[:space:]]*[^[:space:]]/) {
				sub(/^.*size-gate: intrinsic —[[:space:]]*/, ""); print; exit
			}' "$path")
			if [ -n "$reason" ]; then
				echo "intrinsic: $path: $head_count production lines — $reason"
			else
				echo "warn: $path: $base_count -> $head_count production lines (over 1500; plan a split WI or a reviewed size-gate: intrinsic marker)"
			fi
		fi
	else
		echo "$path: new file with $head_count production lines > 1500"
		failed=1
	fi
done < "$scratch/changed"

(
cd desktop/src-tauri/src || exit 1
# temple must not reach merc internals (H4)
! grep -rn 'crate::mercenary::' temple/ || { echo "temple -> mercenary import"; exit 1; }
# ssot may name only slice types from features (H2). Join use statements and
# expand grouped paths before checking; retain the grep for non-use paths.
grep -rnE 'crate::(temple|mercenary)::' ssot*.rs ssot/ 2>/dev/null \
	| grep -vE '::(slice::TempleSlice|MercenarySlice|sync::MercSyncStatus)' && { echo "ssot -> feature internals"; exit 1; }
if ! awk '
function check(path) {
	if (path ~ /(^|::)(temple|mercenary)(::|$)/ &&
	    path !~ /(^|::)temple::slice::TempleSlice$/ &&
	    path !~ /(^|::)mercenary::MercenarySlice$/ &&
	    path !~ /(^|::)mercenary::sync::MercSyncStatus$/) bad=1
}
function group(prefix, path, token) {
	path=prefix
	while (pos<=n) {
		token=tokens[pos++]
		if (token=="as") { pos++; continue }
		else if (token=="{") { group(path); path=prefix }
		else if (token=="}") { if (path!=prefix) check(path); return }
		else if (token=="," || token==";") { if (path!=prefix) check(path); path=prefix }
		else path=path token
	}
}
function inspect(statement) {
	sub(/^.*use[[:space:]]+/, "", statement)
	gsub(/[{},;]/, " & ", statement)
	n=split(statement, tokens, /[[:space:]]+/); pos=1
	group("")
}
FNR==1 { joined=""; collecting=0 }
collecting || /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?use[[:space:]]/ {
	joined=joined " " $0; collecting=1
	if (/;/) { inspect(joined); joined=""; collecting=0 }
}
END { exit bad }
' ssot*.rs ssot/*.rs; then
	echo "ssot -> feature internals"
	exit 1
fi
# lab reads only the frame the read key hands it (POE-391)
! grep -rn 'capture_screen' lab_scan.rs lab/ font_*.rs || { echo "lab code grabs the screen outside read_hotkey"; exit 1; }
# only settings/ and persist_settings may write the settings file (H3)
! grep -rn 'settings::save(' --include='*.rs' . | grep -v '^./settings/' | grep -v 'fn persist_settings' \
	|| { echo "settings::save outside settings/ and persist_settings"; exit 1; }
) || failed=1
exit "$failed"
