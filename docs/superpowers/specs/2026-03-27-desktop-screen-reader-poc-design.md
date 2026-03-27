# Desktop App: Screen Reader PoC

**Epic**: POE-80
**Date**: 2026-03-27
**Status**: Design approved

## Goal

Prove that a Tauri desktop app can read the 3 gem names from the Font of Divine Skill pick screen in PoE1, and send them to the existing Go server to auto-fill the web comparator.

## Tech Stack

- **Tauri v2** — Rust backend + SvelteKit frontend
- **OCR**: OS-native engines
  - Windows: `Windows.Media.Ocr` (zero dependency)
  - macOS: `Vision` framework (zero dependency)
  - Linux: Tesseract fallback (package manager install)
- **PoC targets Windows only** — architecture abstracted via traits for cross-platform later

## Architecture

### Two Input Channels

1. **Client.txt watcher** — tails the PoE log file (~200ms poll interval), near-zero resources. Drives lab state transitions.
2. **Screen reader** — captures a screen region + OCR. Only activates when Font UI is detected. Expensive but short-lived (~30-60s per lab run).

### Lab State Machine

```
IDLE
  | Log: "InstanceClientLabyrinthCraftResultOptionsList recieved"
  | → activate screen reader
  v
PICKING_GEMS
  | Screen: OCR reads gem name from tooltip on hover
  | Deduplicate by name, collect up to 3 unique gems
  | Send each to Go server as detected
  v
CONFIRMED
  | Screen: detect gem selection confirmed
  | Check for more Font uses
  |   ├─ Log: another "InstanceClientLabyrinthCraftResultOptionsList" → PICKING_GEMS
  |   └─ Log: zone change (area "2_10_town" or similar) → DONE
  v
DONE → IDLE
```

Screen reader stays active from first `InstanceClientLabyrinthCraftResultOptionsList` until zone change.

### Client.txt Signals

From real Merciless Lab logs (2026-03-23):

| Signal | Log line | Meaning |
|--------|----------|---------|
| Lab progression | `You have entered Aspirant's Trial.` | Izaro fight room entered |
| 3rd fight area | `Generating area "EndGame_Labyrinth_boss_2_end"` | Final Izaro room |
| Font opened | `InstanceClientLabyrinthCraftResultOptionsList recieved` | Font UI showing options |
| Left to town | `Generating area "2_10_town"` | Lab run ended |

Key insight: `InstanceClientLabyrinthCraftResultOptionsList` fires each time the Font shows options. No need to parse Izaro voicelines (there are many variants). The Font log line is the reliable trigger.

### Screen Reading: What and Where

From game screenshots analysis:

- **Gem names are NOT visible** in the 3-gem pick row — only icons shown
- **Names appear on hover** as the top golden line of a tooltip
- Tooltip position: upper-left area of the Divine Font panel, consistent across hovers
- Gem name format: `"Base Name of Transfigured Variant"` (e.g., "Earthquake of Fragility")
- User hovers all 3 gems ~99% of the time before picking

### Gem Name Matching

OCR result → fuzzy match against known gem name list:

- **229 transfigured gem names** in current database (`gem-colors.json`)
- Closest name pairs are ~93% similar (e.g., "Vaal Impurity of Ice" / "Vaal Impurity of Fire")
- OCR won't confuse visually distinct words ("Ice" ≠ "Fire", "Stone" ≠ "Flame")
- Typical OCR errors: `l`↔`I`↔`1`, `O`↔`0`, `rn`→`m` — all handled by fuzzy match

**Strategy**: Pick best match above minimum threshold (~80%), require significant gap between best and second-best match. Context-aware candidate pool: Font = transfigured only, Dedication Lab = includes Vaal/corrupted.

## Server Integration

### Flow

```
Desktop App                    Go Server                   Web Browser
    |                              |                           |
    |-- POST /api/desktop/gems --> |                           |
    |   { pair: "A7X3",           |                           |
    |     gems: ["Gem A"] }       |                           |
    |                              |-- Mercure event --------> |
    |                              |   topic: poe/desktop/A7X3 |
    |                              |                           |
    |                              |   Comparator auto-fills   |
```

### Pairing (Desktop ↔ Web Browser)

1. Desktop app generates a short pairing code on startup (e.g., `A7X3`)
2. Shows a clickable link: `{server_url}/lab?pair=A7X3` (configurable — `profitofexile.localhost` in dev, production domain in release)
3. User clicks → browser opens → web app reads `pair` from URL → saves to localStorage
4. Web comparator subscribes to Mercure topic `poe/desktop/A7X3`
5. Zero manual input required

## Distribution

### Builds
- **Installer**: `.msi` (Windows), `.dmg` (macOS), `.AppImage`/`.deb` (Linux)
- **Portable**: standalone `.exe` — no installation needed
- **Hosting**: GitHub Releases (public repo, free)

### Auto-Update
- Tauri v2 built-in updater plugin (`@tauri-apps/plugin-updater`)
- Checks GitHub Releases on startup + periodically (~hourly)
- Differential download, one-click update
- Works with both installer and portable versions

### Dev/Test Workflow
- **Dev loop**: Cross-compile `.exe` on dev machine → optional auto-copy to a configurable destination (e.g., shared folder, cloud sync)
  - `make desktop-build` builds; `make desktop-deploy` copies to `DESKTOP_DEPLOY_DIR` (set in `.env.local`, gitignored)
- **Releases**: GitHub Actions builds Windows `.exe` on push/tag for distribution to users
- No dev tooling needed on the testing machine

## Project Structure

```
desktop/
├── src-tauri/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── log_watcher.rs      # Client.txt tail -f
│       ├── capture.rs          # Screen capture (trait + Windows impl)
│       ├── ocr.rs              # OCR engine (trait + Windows.Media.Ocr impl)
│       ├── gem_matcher.rs      # Fuzzy matching against known gem names
│       └── lab_state.rs        # State machine for lab progression
├── src/                        # SvelteKit frontend (minimal PoC UI)
│   └── routes/
│       └── +page.svelte        # Shows status, pairing code, detected gems
├── package.json
└── tauri.conf.json
```

## PoC Scope

### In scope
- Tauri app skeleton with Client.txt tailing
- `InstanceClientLabyrinthCraftResultOptionsList` detection → screen capture activation
- OCR gem name reading from tooltip region
- Fuzzy match against known gem list
- Send matched gem names to Go server
- Server pushes via Mercure → web comparator auto-fills 3 gems
- Pairing via clickable link
- GitHub Actions CI producing Windows `.exe`

### Not in PoC (future work)
- Font empty detection via screen reading (use zone change as stop signal)
- Enchantment option reading for statistics collection
- Multi-use Font tracking (treat each log trigger as fresh)
- Linux/macOS OCR implementations (traits ready, no impl)
- Dedication Lab support (different gem pool)

## Resource Profile

| State | CPU | Screen capture | Duration |
|-------|-----|---------------|----------|
| IDLE (no lab) | ~0% | None | Hours |
| Tailing Client.txt | ~0% | None | Minutes (lab run) |
| Screen reading (Font) | Low-moderate | Small region OCR | 30-60 seconds |

The app is effectively invisible until the Font opens.
