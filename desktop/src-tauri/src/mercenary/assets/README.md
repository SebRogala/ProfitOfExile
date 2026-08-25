# Merc module assets

Files committed here are `include_str!`/`include_bytes!`-embedded by the module —
they ship inside the binary, so a change needs a rebuild.

| File | Provenance |
|---|---|
| `npc-denylist.txt` | The 24 non-mercenary `<Name>, the <Epithet>` dialogue speakers — measured from Sebastian's Client.txt, 2026-08-25 (POE-198). Extend WITHOUT a rebuild by writing the same one-name-per-line format to `<app_data>/merc-npc-denylist.txt`; `mercenary::trigger::NpcDenylist::load` merges it over this list. |
