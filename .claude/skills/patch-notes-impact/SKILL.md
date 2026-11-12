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
- **Name sweep (step 6).** Dispatch one sub-agent and pass `model: "opus"`.
  This is the one exception to the scout model rule above; keep the area
  scouts on sonnet. The owner chose Opus on 2026-11-12, after a four-model
  trial on the same brief. Brief the sweeper as a read-only worker that must
  not re-delegate. Give it the paths of the notes, the external docs, the
  report, and the coverage list, and name `docs/PATCH-NOTES-IMPACT.md` step 6
  for the output shape. When tracker tasks exist, give it each task's id and
  title as the coverage list.
- **Owner approval (step 7).** Ask with AskUserQuestion before any tracker
  write.
- **Tracker writes (step 7).** Load the `pipeforge:tracker` skill before the
  first `pipeforge tracker` call, or run `/pipeforge:breakdown` for the epic.
