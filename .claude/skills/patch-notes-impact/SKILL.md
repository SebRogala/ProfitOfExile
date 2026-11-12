---
name: patch-notes-impact
description: >-
  Find what a new Path of Exile league or patch breaks or changes in this
  product, and propose a tracker epic with one task per change. Use when the
  owner pastes a pathofexile.com patch-notes forum thread URL, or says "new
  league notes are out", "run the patch notes", "what does this patch break",
  "impact of the league patch", "go through the patch notes". Not for the
  league schema rollover (docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md).
---

# Patch-notes impact pass

Follow `docs/PATCH-NOTES-IMPACT.md` step by step. That file holds every rule of
the pass. This skill adds only the Claude Code mechanics below.

## Claude Code mechanics

- **Scratch directory.** Fetch into this session's scratchpad directory, in its
  own new subdirectory.
- **Scouts (step 3).** Dispatch one sub-agent per relevant section, in one
  message so they run in parallel. Pass `model: "sonnet"` on every scout;
  never run a scout on Fable. Brief each scout as a worker that must not
  re-delegate. Give it the section file path, its mapped areas, and the path
  `docs/PATCH-NOTES-IMPACT.md` for the hit format.
- **External sheet tabs (step 4).** Fetch a CSV export URL with WebFetch, or
  the `web-fetch` agent when WebFetch is absent.
- **Owner approval (step 6).** Ask with AskUserQuestion before any tracker
  write.
- **Tracker writes (step 6).** Load the `pipeforge:tracker` skill before the
  first `pipeforge tracker` call, or run `/pipeforge:breakdown` for the epic.
