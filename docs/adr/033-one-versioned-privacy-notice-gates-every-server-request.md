---
uid: 11c1a16b-334d-4c46-80e0-f79bb7f3162d
---

# ADR-033: One Versioned Privacy Notice Gates Every Server Request

## Status

Accepted 2026-11-13 (POE-395). Last verified 2026-11-13.

## Enforcement

Tier: required

Scope: The Data & Privacy notice source files, every request from the desktop
app to the ProfitOfExile server (`desktop/src-tauri`, `desktop/src`), and the
website `/privacy` page (`frontend/`).

Summary: One versioned notice file is the only notice text. The desktop app
sends no request to our server until the user has accepted the current
revision; a material change bumps the revision and asks again.

## Context

The desktop app sends data to the ProfitOfExile server (trade lookups,
device identity, usage). Users must be told what is processed before any of
it leaves their machine, and the website must show the same text the app
shows. The desktop and frontend builds cannot read files outside their own
trees (`desktop/Dockerfile` build context, the Makefile rsync), so a single
top-level file is not reachable by both builds.

## Decision

### One source

- Canonical file: `desktop/src/lib/privacy/notice.json`.
- Mirror: `frontend/src/lib/privacy/notice.json`, byte-identical;
  `mirror-identity.test.ts` enforces the identity.
- Shape: `revision` (integer), `date`, `draft` (bool), `changes[]`
  (`{revision, date, items[]}`), `sections[]`
  (`{id, heading, paragraphs[], items[]}`).
- Rust embeds the file with `include_str!` and reads only `revision`. The
  desktop modal and the website `/privacy` page each render the body with
  their own renderer.

This ADR is the normative rule; the notice file is the normative text.
Neither restates the other.

### Material vs wording-only edits

- **Material:** any change to what is collected, its source, purpose,
  recipient, retention, legal basis, or the controller. Any server-side
  change in what is collected is material, even when no desktop code
  changes.
- A material edit bumps `revision`, appends a `changes[]` entry, and ships
  with a desktop release.
- **Wording-only:** the same facts in clearer text. It keeps `revision` and
  adds no `changes[]` entry.
- When in doubt, treat the edit as material.

### Gate

- No request to our server before the current revision is accepted. This
  covers the Rust `server_http` client and the webview's server URL.
- The modal is a notice of the processing the app needs (Continue / Quit),
  not a consent request.
- Quit exits the app; the next start shows the notice again. There are no
  per-feature opt-outs.
- Accepted means the stored revision is greater than or equal to the current
  revision.
- When `settings.json` is unreadable or not JSON, an acceptance holds for the
  current session only: the app prompts again on the next launch and never
  unblocks because of a read error. A file that is valid JSON but fails the
  full parse resets to defaults on the first save
  ([ADR-031](031-proxy-one-setting-every-transport-fails-closed.md)
  Consequences), which drops the acceptance and prompts again; that is the
  safe direction.
- A revision bump shows the "Data & privacy changes" modal, which lists the
  `changes[]` entries newer than the accepted revision and requires
  acceptance again.

### Scope

Acceptance is enforced at exactly two gates: the server HTTP client and
`capture::capture_screen`, the only screen-grab path, which grabs nothing
while the notice is not accepted. `Client.txt` reading, user-initiated GGG
trade requests and the GitHub update check stay outside (Operator ruling
2026-11-13). Readers, features and hotkeys carry no acceptance checks of
their own. Widening the gate further needs a new decision.

## Relations

- [ADR-031](031-proxy-one-setting-every-transport-fails-closed.md): the same
  one-application-point, fail-closed shape; non-acceptance is a second reason
  to refuse a server request.
- [ADR-013](013-ui-picks-persist-in-a-schema-less-prefs-map.md): the accepted
  revision is a typed settings field, not a prefs-map entry, because Rust
  reads it.

## Consequences

- Every material notice change costs a desktop release, including changes
  driven only by the server.
- Users who quit at the notice cannot use any server-backed feature.
- The two notice files must be edited together; the mirror test fails
  otherwise.
