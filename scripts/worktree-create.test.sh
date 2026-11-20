#!/usr/bin/env bash
# Exercises `make worktree-create` against a throwaway git repository and a fake
# `pipeforge` on PATH that logs its argv. Needs git, make and bash only.
# Run: bash scripts/worktree-create.test.sh
set -euo pipefail

src=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
check() { # check <description> <command...>
	local desc=$1; shift
	if "$@"; then echo "ok   - $desc"; else echo "FAIL - $desc"; fail=1; fi
}

mkdir -p "$work/bin"
cat >"$work/bin/pipeforge" <<'FAKE'
#!/usr/bin/env bash
echo "$*" >>"$FAKE_LOG"
FAKE
chmod +x "$work/bin/pipeforge"
export PATH="$work/bin:$PATH" FAKE_LOG="$work/pipeforge.log"

fresh_repo() { # fresh_repo <dir>: one commit on dev with the Makefile and scripts
	local r=$1
	: >"$FAKE_LOG"
	git init -q -b dev "$r"
	mkdir -p "$r/scripts/lib"
	cp "$src/Makefile" "$r/"
	cp "$src/scripts/worktree-create.sh" "$r/scripts/"
	cp "$src/scripts/lib/lane-name.sh" "$r/scripts/lib/"
	git -C "$r" add .
	git -C "$r" -c user.name=t -c user.email=t@t commit -qm base
}
branches() { git -C "$1" for-each-ref --format='%(refname:short)' refs/heads/; }

# keep:happy (AC-2) — the lane is a worktree on its own branch at dev, then pipeforge init runs in it.
r="$work/happy"
fresh_repo "$r"
check "happy: make worktree-create exits 0" make -s -C "$r" worktree-create name=lanexyz
check "happy: lane is on branch lanexyz" test "$(git -C "$r/.worktrees/lanexyz" rev-parse --abbrev-ref HEAD 2>/dev/null)" = lanexyz
check "happy: lane HEAD equals dev" test "$(git -C "$r/.worktrees/lanexyz" rev-parse HEAD 2>/dev/null)" = "$(git -C "$r" rev-parse dev)"
check "happy: pipeforge init ran once in the lane" test "$(cat "$FAKE_LOG")" = "init --cwd $r/.worktrees/lanexyz"

# keep:critical (AC-3) — a name holding a quote and shell syntax must run nothing:
# an interpolating recipe would execute arbitrary commands in the caller's shell,
# and an off-pattern name would leave a half-made lane mapped to another slug.
r="$work/critical"
fresh_repo "$r"
check "critical: hostile name exits nonzero" bash -c '! make -s -C "$1" worktree-create "name=lane\"; touch $2/PWNED; \"" 2>/dev/null' _ "$r" "$work"
check "critical: injected command did not run" test ! -e "$work/PWNED"
check "critical: no .worktrees entry" test -z "$(ls -A "$r/.worktrees" 2>/dev/null)"
check "critical: no new branch" test "$(branches "$r")" = dev
check "critical: pipeforge was not called" test ! -s "$FAKE_LOG"

exit "$fail"
