# Desktop Release

Release a new version of the desktop app. Bumps version in all required files, commits, tags, and pushes.

## Usage
```
/release <version>
```
Example: `/release 0.1.2`

## Steps

1. **Parse the version** from the argument (e.g., `0.1.2`). If no argument provided, ask the user.

2. **Bump version** in all three files:
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

6. **Push** (ask user for confirmation first — push auto-deploys server):
   ```
   git push origin main --tags
   ```

7. **Confirm** the tag was pushed and CI should trigger the desktop build.

## Important
- Tag format MUST be `v-desktop-X.Y.Z` — CI only triggers on this pattern
- All three version files MUST match — Tauri updater compares installed version against update manifest
- Always ask before pushing — push to main auto-deploys the server
