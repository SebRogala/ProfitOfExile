# Patch-notes impact pass

**Status:** current procedure.

**Last verified:** 2026-11-12 against `scripts/fetch-patch-notes.py` at `c188cce3`.

**Canonical for:** how a league's patch notes become an impact report and a
tracker epic with one task per change. Agent-host adapters (for example
`.claude/skills/patch-notes-impact/SKILL.md`) reference this file and add only
host-specific mechanics.

Run this pass once per patch-notes thread: a new league, or a major patch the
owner names. It does not replace the
[league rollover runbook](LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md).

## Inputs

- **The threads.** One or more `https://www.pathofexile.com/forum/view-thread/<id>`
  URLs: the patch notes, plus any announcement thread for the same league, such
  as "New and Changed Gems". The rest of this file calls all of them "the notes".
- **The inventory.** What the product depends on, from three sources:
  1. [GAME-FACTS.md](GAME-FACTS.md) — the dated game facts the code treats as
     invariants.
  2. Every guide listed under "Current guides and narratives" in the
     [documentation index](README.md), such as the temple, mercenaries, boss
     loot, lab font, trade, collector and analysis-cache guides.
  3. The code itself. GAME-FACTS misses dependencies nobody documented, so
     every section is also grepped in `desktop/`, `frontend/`, `internal/`,
     `cmd/`, `db/` and `data/`.
- **External change docs.** Any owner-supplied document on one area's changes,
  for example a community Google Sheet tab on Incursion changes. Export a
  public sheet tab as CSV with
  `https://docs.google.com/spreadsheets/d/<id>/export?format=csv&gid=<gid>`.
  Ask the owner for these before step 4; the owner may have none.

## Precedence

For an area an external change doc covers, the external doc wins over the
agent's reading of the notes. Record every disagreement between the two in the
report's Conflicts section, quoting both sides. Never resolve a conflict
silently; the owner rules on each one.

## Procedure

### 1. Fetch

Fetch each thread into its own subdirectory of a scratch directory outside the
repository. The notes are third-party content and stay out of git.

```
python3 scripts/fetch-patch-notes.py <thread URL> <scratch dir>
```

The fetcher saves the thread's first post only, in one of two layouts:

- **Patch notes:** `00-index.txt` plus one `NN-<slug>.txt` per
  table-of-contents section. League sub-headings become `###` lines. Read
  `00-index.txt` first; it lists each section file with its line count.
- **"New and Changed Gems":** a first post holding `id="gem-forum-post"`
  produces `post.txt` and `gems.tsv`. `gems.tsv` has one row per gem, with the
  columns `name`, `status` (`new` or `updated`), `icon_url` and `diff`. Treat
  each `gems.tsv` row as one section for steps 2 and 3.

Exit codes:

- `0` — written; stdout lists the paths.
- `1` — the forum markup was not recognised, and nothing was written. stderr
  names the broken assumption. A network failure also exits `1`, with a
  Python traceback. Fix the fetcher or retry; do not hand-scrape the page.
- `2` — the URL is not a pathofexile.com forum thread, or the arguments are
  wrong.

### 2. Map sections to product areas

The product areas are the `##` sections of GAME-FACTS
(`grep -n '^## ' docs/GAME-FACTS.md`) plus the current guides in the
documentation index. Map each section file to the areas it can touch. Within a
large section, map each `###` sub-heading on its own.

Skip a section only when no product area could depend on it: cosmetic
microtransactions, Ruthless-only changes, console-only changes. Give each skip
a one-line reason in the report. When in doubt, keep the section.

### 3. Scout each relevant section

Give each relevant section to its own scout, where the agent host supports
delegation; otherwise work through the sections one by one. Each scout gets the
section file, the areas mapped to it, and this file.

A scout reports every hit, including no-impact ones; the report filters, not
the scout. A hit is one note line matched against one product dependency. For
each hit the scout:

1. Quotes the note line verbatim, with its section file.
2. Names the affected dependency: a GAME-FACTS entry, a doc section, or code at
   `file:line`. It greps the code for the note's names (items, mechanics, room
   names, log wordings, UI text) before concluding no code depends on it.
3. Classifies the hit, one of:
   - `breaks` — shipped behavior becomes wrong once the patch lands.
   - `needs-update` — a fact, doc, value table or label goes stale; behavior
     still works.
   - `opportunity` — the change enables something the product could add.
   - `no-impact` — checked, and nothing depends on it.
4. Marks the hit `observed` (the note or the code states it) or `inferred`
   (given X, likely Y; state X).

Hit format (illustrative values):

```
- section: 03-incursion-and-atziri.txt
  note: "Architects can no longer be found in the Apex of Atzoatl."
  affects: docs/GAME-FACTS.md § Temple of Atzoatl, desktop/src/lib/temple/values.ts:<line>
  class: breaks
  basis: inferred — given the room-value table assumes an Apex architect, likely its tier math is wrong
  external: null
```

`external` is `null` unless step 4 applies an external doc to the hit.

For a `gems.tsv` row, `section` is `gems.tsv:<name>` and `note` is the row's
`diff`. For a new gem, `note` quotes the gem's name and status.

### 4. Apply the external docs

For each external change doc, match its rows to the hits in its area. Where the
doc confirms a hit, cite it in `external`. Where the doc adds a change the notes
lack, add a hit with the doc as its source. Where the doc contradicts a hit,
the doc wins and the contradiction goes to Conflicts (see Precedence).

### 5. Consolidate the impact report

Write the report in the scratch directory, with these sections in order:

1. **Breaks** — every `breaks` hit.
2. **Needs update** — every `needs-update` hit.
3. **Opportunities** — every `opportunity` hit.
4. **Conflicts** — each disagreement between an external doc and the notes.
5. **GAME-FACTS changes** — each entry the patch changes, with old and new.
6. **No impact** — one line per hit.
7. **Skipped sections** — one line per skip, with its reason.

Merge hits that need the same fix into one change. Keep `observed` and
`inferred` on every merged line.

### 6. Sweep the repository for every changed name

Run one dedicated name sweep after the report and before the epic. The sweep
catches dependencies that cross product areas, which a scout reading one
section misses. It does not replace the scouts in step 3.

The sweep reads the notes, every external change doc, and the coverage list.
The coverage list is one line per change under Breaks, Needs update and
GAME-FACTS changes in the report, with its title and affected files. Each such
change becomes one task in step 7. When tracker tasks for the patch already
exist, the coverage list is each task's id and title instead.

1. **Extract names.** Take every proper name from each note line that removes,
   renames, converts, adds, moves or re-values something. Names cover items,
   currencies, rooms, gems, bosses, NPCs, mechanics, stash-tab categories and
   item classes, such as "Scarab". Take both the old and the new name of a
   rename. Skip cosmetic and Ruthless-only lines.
2. **Grep each name across the whole repository.** Run from the repository
   root:

   ```
   git grep -n -i -F "<name>"
   ```

   `-i -F` already matches a name's plural and possessive. Grep three
   variants the same way: the metadata-id form without spaces (for example
   `AfflictionOrb`), the singular of a plural name, and the form with an
   ASCII apostrophe in place of a typographic one.
3. **Assign each hit file to the change that covers it.** A change covers a
   file only when the change's scope includes that file. A name match alone is
   not coverage. Mark each hit that no change covers `UNCOVERED`.
4. **Report in the fixed shape below.** List the names with zero hits in one
   line, not one line each. Order UNCOVERED lines with hits that break
   shipped behavior first. A Doubt is a hit the sweeper cannot place in a
   change or in No impact. Write at most 5 Doubts.

Sweep output (illustrative values):

```
### Summary
- names extracted: 41; names with repo hits: 23; hit files: 57; UNCOVERED hits: 6

### UNCOVERED
Vial | removed | internal/collector/<file>:<line> | the collector still fetches vial prices | NEW

### Covered
Vial | removed | 3 | Temple: remove vials, Docs: update GAME-FACTS

### Zero-hit names
<name>, <name>

### Doubts
- <one line per doubt; none when there are none>
```

An UNCOVERED line is `name | change | file:line | why it matters | covering
change or NEW`, one per uncovered `file:line`. Write at most 3 lines for one
name in one file, then one `<name> | <file> | and N more` line. A Covered line
is `name | change | <hit-file count> | <covering changes, comma-separated>`.
Name a covering change by its title, or by its tracker id once the task
exists.

5. **Verify and fold.** The agent running the pass opens each UNCOVERED hit at
   its `file:line` and confirms the dependency. Drop a false hit, and record
   it under No impact with its reason. Fold each confirmed hit into the
   report: extend the change that should cover it, or add a new change under
   Breaks or Needs update. Give a new change a class and a basis, as for a
   scout hit. Mark a change `owner ruling` when its fix is a product decision.
   Resolve each Doubt the same way, or mark it `owner ruling`.

### 7. Propose the epic and its tasks

Propose one epic for the patch and one task per change from Breaks, Needs
update and GAME-FACTS changes. List Opportunities separately; the owner picks
which become tasks.

Each new gem gets a task too. The task adds the gem's icon URL to
`internal/icons/urls/gems.json`, pulls it into the icon cache and seeds
production, per
[ICONS.md](ICONS.md) § "A new gem poewiki does not have yet". The same pass
lists every `web.poecdn.com` entry already in `gems.json` as a pending swap
under that section. When the product prices or tracks that gem's type, the
same task adds that check.

Each task carries:

- the note quote and its section file;
- the affected files, docs and GAME-FACTS entries;
- the source: the notes, an external doc, or both;
- the class and the `observed` / `inferred` basis.

Show the list to the owner, and ask for a ruling on each change marked
`owner ruling`. Create the epic and tasks in the YouTrack project
`POE` only after the owner approves the list. Use the `pipeforge tracker` CLI,
or the pipeforge breakdown workflow for the epic. Tracker descriptions are the
canonical specification for each change (see [AGENTS.md](../AGENTS.md)).

### 8. GAME-FACTS updates

Every GAME-FACTS entry the patch changes gets its own task in the epic. The
pass itself edits no GAME-FACTS entry. The task updates the entry, its date
and its source (the patch-notes thread). An `inferred` change moves the entry
to "Not validated" until someone measures it in game.
