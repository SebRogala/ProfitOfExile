# Desktop Release

Release a new version of the desktop app. Bumps version in all required files, commits, tags, and pushes.

## Usage
```
/release <version>
```
Example: `/release 0.1.2` (stable), `/release 0.1.3-beta.1` (beta channel).

## Steps

1. **Parse the version** from the argument (e.g., `0.1.2`, or `0.1.3-beta.1`
   for a beta). If no argument provided, ask the user. A version carrying a
   `-<prerelease>` suffix ships on the beta channel — see step 5.

2. **Bump version** in all three files — the `-beta.N` suffix is part of the
   version and goes into all three verbatim:
   - `desktop/src-tauri/tauri.conf.json` → `"version": "<version>"`
   - `desktop/src-tauri/Cargo.toml` → `version = "<version>"` (first occurrence under `[package]`)
   - `desktop/package.json` → `"version": "<version>"`

3. **Update Cargo.lock** by running:
   ```
   cd desktop/src-tauri && cargo check --message-format=short 2>/dev/null; cd -
   ```
   (This regenerates Cargo.lock with the new version. Skip if cargo is not available locally.)

4. **Commit** the version bump:
   ```
   git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/Cargo.toml desktop/src-tauri/Cargo.lock desktop/package.json
   git commit -m "chore(desktop): bump version to v<version>"
   ```

5. **Tag** with the correct format:
   ```
   git tag v-desktop-<version>
   ```
   Stable: `v-desktop-X.Y.Z`. Beta: `v-desktop-X.Y.Z-beta.N` — same tag prefix,
   same workflow. CI publishes a beta tag as a GitHub **prerelease** (so
   `/releases/latest`, which stable devices poll, keeps pointing at the newest
   stable release) and additionally re-publishes that build's `latest.json`
   onto the rolling `desktop-beta` release, which is the fixed URL beta devices
   poll. Details in docs/DEPLOY.md → *Desktop release channels*.

6. **Push** (ask user for confirmation first — push auto-deploys server).

   **Ordering warning — check before pushing:** if this desktop version calls a
   server API that is not in production yet, or if the release contains a change
   to `internal/mercenary/families.go` (the compiled families gate — it refuses
   uploads naming a family it does not carry, and it moves for a GROWTH and for
   a SHRINK alike) or to `vocab.rs`'s `FAMILY_ALIASES` (which changes what the
   desktop derives without the fixture moving at all), the server must be LIVE
   first. Merged is not live: the Coolify swap takes minutes and
   the deploy step is fire-and-forget. Push the server commit, verify it landed
   (docs/DEPLOY.md → *Verifying a deploy landed*), then push the tag.

   The same order — server first — also applies when the change SHRINKS the
   family set (an alias folding two names into one, a support GGG removed).
   The server then refuses uploads of the dropped name from older desktops,
   which is intended: that art can never be matched again.

   ```
   git push origin main --tags
   ```

7. **Confirm** the tag was pushed and CI should trigger the desktop build.

## Important
- Tag format MUST be `v-desktop-X.Y.Z` (stable) or `v-desktop-X.Y.Z-beta.N`
  (beta) — CI only triggers on the `v-desktop-` prefix, and any `-<prerelease>`
  suffix routes the build to the beta channel instead of stable
- All three version files MUST match — Tauri updater compares installed version against update manifest
- Always ask before pushing — push to main auto-deploys the server
- Beta testers are not a build concern: a device gets beta updates (and the
  hidden modules) when its server-side role is `editor`/`admin`, set with
  `/promote` on the server. No per-tester build, no fingerprint in the repo.
  Procedure and the public-repo rules: docs/DEPLOY.md → *Who is on the beta
  channel*.
- The repository and every release page are public. Release notes are
  generated from commit subjects, for prereleases too — never put a tester's
  alias/fingerprint/name in a commit, tag, or release note, and don't describe
  a beta feature in a commit as if the gate made it private (it doesn't).
- First beta tag ever: three things are unproven until it runs — GitHub's
  `/releases/latest` skipping the prerelease, the rolling `desktop-beta`
  release accepting an overwritten `latest.json`, and NSIS building a
  `-beta.N` version. Check the release page and a stable device's update
  check afterwards; do not assume.
