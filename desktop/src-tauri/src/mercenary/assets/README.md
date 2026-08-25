# Merc module assets

Files committed here are `include_str!`/`include_bytes!`-embedded by the module —
they ship inside the binary, so a change needs a rebuild.

| File | Provenance |
|---|---|
| `npc-denylist.txt` | The 25 non-mercenary `<Name>, <joiner> <Epithet>` dialogue speakers (all of them use `the`; the trigger's shape accepts ANY lowercase joiner, so an NPC introduced with `of` belongs here too) — measured from Sebastian's Client.txt, 2026-08-25, rescanned with the `, <lowercase joiner> ` shape (POE-198). Extend WITHOUT a rebuild by writing the same one-name-per-line format to `<app_data>/merc-npc-denylist.txt`; `mercenary::trigger::NpcDenylist::load` merges it over this list. |
